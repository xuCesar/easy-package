//! 可执行文件验证：路径白名单、指纹（含内容摘要）与 npm 执行上下文预检。
use std::{
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::SystemTime,
};

use crate::{
    adapters::runner::{execution_trust, redact_and_truncate, CommandRunner},
    error::AppError,
    models::{ExecutionTrust, PackageManagerId},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ExecutableFingerprint {
    pub(super) canonical_path: PathBuf,
    pub(super) size: u64,
    pub(super) modified_at: Option<SystemTime>,
    pub(super) content_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NpmExecutionContext {
    pub(super) node_fingerprint: ExecutableFingerprint,
    pub(super) prefix: PathBuf,
    pub(super) cache: PathBuf,
}

pub(super) fn validate_homebrew_executable(executable: &Path) -> Result<(), AppError> {
    let canonical = executable
        .canonicalize()
        .map_err(|error| AppError::Command(format!("无法验证 Homebrew 路径：{error}")))?;
    let allowed = [
        Path::new("/opt/homebrew/bin/brew"),
        Path::new("/usr/local/bin/brew"),
    ];
    if allowed
        .iter()
        .any(|path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf()) == canonical)
    {
        Ok(())
    } else {
        Err(AppError::Command(
            "Homebrew 写操作仅允许受信任的标准安装路径".into(),
        ))
    }
}

pub(super) fn action_environment(manager_id: PackageManagerId) -> Vec<(String, String)> {
    if manager_id == PackageManagerId::Homebrew {
        vec![
            ("HOMEBREW_NO_AUTO_UPDATE".into(), "1".into()),
            ("HOMEBREW_NO_ANALYTICS".into(), "1".into()),
            ("HOMEBREW_NO_ENV_HINTS".into(), "1".into()),
        ]
    } else {
        Vec::new()
    }
}

pub(super) fn capture_executable_fingerprint(
    path: &Path,
) -> Result<ExecutableFingerprint, AppError> {
    let canonical_path = path
        .canonicalize()
        .map_err(|error| AppError::Command(format!("无法验证包管理器路径：{error}")))?;
    let metadata = canonical_path
        .metadata()
        .map_err(|error| AppError::Command(format!("无法读取包管理器文件信息：{error}")))?;
    if !metadata.is_file() {
        return Err(AppError::Command("包管理器路径不是普通文件".into()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o002 != 0 {
            return Err(AppError::Command(
                "包管理器可执行文件允许任意用户写入，拒绝用于写操作".into(),
            ));
        }
    }
    Ok(ExecutableFingerprint {
        content_digest: digest_file_contents(&canonical_path)?,
        size: metadata.len(),
        modified_at: metadata.modified().ok(),
        canonical_path,
    })
}

// 指纹除 size/mtime 外附带内容摘要，缩小校验与 spawn 之间的 TOCTOU 窗口：
// 原地替换同尺寸、同 mtime 的文件也会被检出。
pub(super) fn digest_file_contents(path: &Path) -> Result<String, AppError> {
    use sha2::{Digest, Sha256};
    let file = std::fs::File::open(path)
        .map_err(|error| AppError::Command(format!("无法读取包管理器可执行文件：{error}")))?;
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| AppError::Command(format!("无法读取包管理器可执行文件：{error}")))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            use std::fmt::Write;
            let _ = write!(hex, "{byte:02x}");
            hex
        }))
}

pub(super) fn verify_executable_fingerprint(
    path: &Path,
    expected: &ExecutableFingerprint,
) -> Result<(), AppError> {
    if &capture_executable_fingerprint(path)? == expected {
        Ok(())
    } else {
        Err(AppError::Command(
            "包管理器可执行文件在计划确认后发生变化，请重新生成计划".into(),
        ))
    }
}

pub(super) fn validate_pnpm_executable(executable: &Path) -> Result<(), AppError> {
    if executable.file_name().and_then(|name| name.to_str()) != Some("pnpm") {
        return Err(AppError::Command(
            "pnpm 写操作仅允许扫描得到的 pnpm 可执行文件".into(),
        ));
    }
    let canonical = executable
        .canonicalize()
        .map_err(|error| AppError::Command(format!("无法验证 pnpm 路径：{error}")))?;
    if !canonical.is_file()
        || !matches!(
            execution_trust(&canonical, &[]),
            ExecutionTrust::System | ExecutionTrust::Managed | ExecutionTrust::UserManaged
        )
    {
        return Err(AppError::Command(
            "pnpm 可执行文件不在允许写操作的可信目录中".into(),
        ));
    }
    Ok(())
}

pub(super) fn validate_npm_executable(executable: &Path) -> Result<(), AppError> {
    if executable.file_name().and_then(|name| name.to_str()) != Some("npm") {
        return Err(AppError::Command(
            "npm 写操作仅允许扫描得到的 npm 可执行文件".into(),
        ));
    }
    let canonical = executable
        .canonicalize()
        .map_err(|error| AppError::Command(format!("无法验证 npm 路径：{error}")))?;
    let npm_trust = execution_trust(&canonical, &[]);
    if !canonical.is_file()
        || !matches!(
            npm_trust,
            ExecutionTrust::Managed | ExecutionTrust::UserManaged
        )
    {
        return Err(AppError::Command(
            "npm 可执行文件不在允许写操作的可信目录中".into(),
        ));
    }
    let node = executable
        .parent()
        .map(|directory| directory.join("node"))
        .ok_or_else(|| AppError::Command("无法定位 npm 关联的 Node.js".into()))?;
    let node_canonical = node.canonicalize().map_err(|_| {
        AppError::Command("npm 同目录缺少 Node.js；为避免 PATH 运行时错配，已拒绝写操作".into())
    })?;
    if !node_canonical.is_file() || execution_trust(&node_canonical, &[]) != npm_trust {
        return Err(AppError::Command(
            "npm 与同目录 Node.js 的信任来源不一致，已拒绝写操作".into(),
        ));
    }
    Ok(())
}

pub(super) fn validate_npm_active_node(
    snapshot: &crate::models::EnvironmentScan,
    executable: &Path,
) -> Result<(), AppError> {
    let sibling = executable
        .parent()
        .map(|directory| directory.join("node"))
        .and_then(|path| path.canonicalize().ok())
        .ok_or_else(|| AppError::Command("无法验证 npm 同目录 Node.js".into()))?;
    let active = snapshot
        .path_observations
        .iter()
        .find(|observation| observation.command == "node")
        .and_then(|observation| observation.active_path.as_deref())
        .map(PathBuf::from)
        .and_then(|path| path.canonicalize().ok())
        .ok_or_else(|| AppError::Command("扫描快照缺少可验证的 PATH Node.js".into()))?;
    if active != sibling {
        return Err(AppError::Command(format!(
            "npm 关联 Node.js 与 PATH 当前 Node.js 不一致：{}；请先解决运行时冲突",
            readable_action_path(&active)
        )));
    }
    Ok(())
}

pub(super) fn npm_preflight(
    runner: &CommandRunner,
    executable: &Path,
) -> Result<(PathBuf, PathBuf, PathBuf), AppError> {
    let node = executable
        .parent()
        .map(|directory| directory.join("node"))
        .ok_or_else(|| AppError::Command("无法定位 npm 关联的 Node.js".into()))?;
    let prefix = npm_config_path(runner, executable, "prefix")?;
    let cache = npm_config_path(runner, executable, "cache")?;
    Ok((node, prefix, cache))
}

pub(super) fn npm_config_path(
    runner: &CommandRunner,
    executable: &Path,
    key: &str,
) -> Result<PathBuf, AppError> {
    let output =
        runner.run_cancellable(executable, &["config", "get", key], &AtomicBool::new(false));
    if !output.success {
        return Err(AppError::Command(format!(
            "无法读取 npm {key}：{}",
            output.combined_output()
        )));
    }
    let value = output.stdout.trim();
    if value.is_empty() || value.lines().count() != 1 {
        return Err(AppError::Command(format!("npm {key} 返回了无效路径")));
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() || !is_allowed_npm_data_path(&path) {
        return Err(AppError::Command(format!(
            "npm {key} 不在允许的用户或受管理目录中：{}",
            redact_and_truncate(value)
        )));
    }
    Ok(path)
}

pub(super) fn is_allowed_npm_data_path(path: &Path) -> bool {
    path.starts_with("/opt/homebrew")
        || path.starts_with("/usr/local")
        || dirs::home_dir().is_some_and(|home| path.starts_with(home))
}

pub(super) fn path_looks_read_only(path: &Path) -> bool {
    path.metadata()
        .ok()
        .or_else(|| path.parent().and_then(|parent| parent.metadata().ok()))
        .map(|metadata| metadata.permissions().readonly())
        .unwrap_or(false)
}

pub(super) fn readable_action_path(path: &Path) -> String {
    let value = path.to_string_lossy().into_owned();
    if let Some(home) = dirs::home_dir() {
        if let Ok(relative) = path.strip_prefix(home) {
            return format!("~/{}", relative.to_string_lossy());
        }
    }
    value
}

pub(crate) fn is_valid_formula_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && !value.contains("..")
        && !value.contains("//")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/@+_.-".contains(&byte))
}

pub(crate) fn is_valid_registry_package_name(value: &str) -> bool {
    if value.is_empty() || value.len() > 214 || value.starts_with('.') || value.contains("..") {
        return false;
    }
    let valid_segment = |segment: &str| {
        !segment.is_empty()
            && segment.len() <= 128
            && segment
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    };
    if let Some(scoped) = value.strip_prefix('@') {
        let mut parts = scoped.split('/');
        matches!((parts.next(), parts.next(), parts.next()), (Some(scope), Some(name), None) if valid_segment(scope) && valid_segment(name))
    } else {
        !value.contains('/') && !value.contains('@') && valid_segment(value)
    }
}
