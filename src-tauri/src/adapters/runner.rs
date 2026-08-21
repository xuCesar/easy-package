use std::{
    collections::HashSet,
    env,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use wait_timeout::ChildExt;

use crate::models::ExecutionTrust;

const OUTPUT_LIMIT: usize = 8_000;

#[derive(Debug, Default)]
pub struct CommandRunner {
    timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    pub fn combined_output(&self) -> String {
        let value = if self.stderr.trim().is_empty() {
            &self.stdout
        } else {
            &self.stderr
        };
        redact_and_truncate(value)
    }
}

impl CommandRunner {
    pub fn default() -> Self {
        Self {
            timeout: Duration::from_secs(20),
        }
    }

    pub fn run_cancellable(
        &self,
        executable: &Path,
        args: &[&str],
        cancelled: &AtomicBool,
    ) -> CommandOutput {
        // 只记录程序名与参数，不记录命令输出，保持既有脱敏边界。
        let trace_started = std::time::Instant::now();
        let output = self.run_cancellable_inner(executable, args, cancelled);
        tracing::debug!(
            program = %executable.display(),
            args = ?args,
            success = output.success,
            exit_code = output.exit_code,
            duration_ms = trace_started.elapsed().as_millis() as u64,
            "外部命令执行"
        );
        output
    }

    fn run_cancellable_inner(
        &self,
        executable: &Path,
        args: &[&str],
        cancelled: &AtomicBool,
    ) -> CommandOutput {
        let mut command = Command::new(executable);
        command
            .args(args)
            .env("NO_COLOR", "1")
            .env("HOMEBREW_NO_AUTO_UPDATE", "1")
            .env("HOMEBREW_NO_ANALYTICS", "1")
            .env("HOMEBREW_NO_ENV_HINTS", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // npm/pnpm 等脚本通过 `/usr/bin/env node` 启动，只为当前子进程补齐可信运行目录。
        if let Some(path) = super::discovery::command_search_path(executable) {
            command.env("PATH", path);
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                return CommandOutput {
                    success: false,
                    exit_code: None,
                    stdout: String::new(),
                    stderr: error.to_string(),
                }
            }
        };

        // 持续读取两个管道，避免大量输出填满 OS 缓冲区后子进程与父进程互相等待。
        let stdout_reader = child.stdout.take().map(read_in_background);
        let stderr_reader = child.stderr.take().map(read_in_background);
        let started_at = Instant::now();
        let (success, exit_code, fallback_error) = loop {
            if cancelled.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                break (false, None, Some("扫描已取消，命令已终止".into()));
            }
            let remaining = self.timeout.saturating_sub(started_at.elapsed());
            if remaining.is_zero() {
                let _ = child.kill();
                let _ = child.wait();
                break (
                    false,
                    None,
                    Some(format!(
                        "命令执行超过 {} 秒，已终止",
                        self.timeout.as_secs()
                    )),
                );
            }
            match child.wait_timeout(remaining.min(Duration::from_millis(200))) {
                Ok(Some(status)) => break (status.success(), status.code(), None),
                Ok(None) => continue,
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break (false, None, Some(error.to_string()));
                }
            }
        };

        let stdout = join_reader(stdout_reader);
        let mut stderr = join_reader(stderr_reader);
        if let Some(error) = fallback_error {
            if !stderr.is_empty() {
                stderr.push('\n');
            }
            stderr.push_str(&error);
        }
        CommandOutput {
            success,
            exit_code,
            stdout,
            stderr,
        }
    }
}

fn read_in_background<R>(mut reader: R) -> thread::JoinHandle<Vec<u8>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut output = Vec::new();
        let _ = reader.read_to_end(&mut output);
        output
    })
}

fn join_reader(reader: Option<thread::JoinHandle<Vec<u8>>>) -> String {
    let bytes = reader
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    String::from_utf8_lossy(&bytes).into_owned()
}

pub fn execution_trust(path: &Path, _common_paths: &[&str]) -> ExecutionTrust {
    execution_trust_with_home(path, dirs::home_dir().as_deref())
}

pub fn can_execute(trust: ExecutionTrust) -> bool {
    !matches!(
        trust,
        ExecutionTrust::Unverified | ExecutionTrust::NotApplicable
    )
}

fn execution_trust_with_home(path: &Path, home: Option<&Path>) -> ExecutionTrust {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if resolved.starts_with("/usr/bin") || resolved.starts_with("/bin") {
        return ExecutionTrust::System;
    }
    if resolved.starts_with("/opt/homebrew") || resolved.starts_with("/usr/local") {
        return ExecutionTrust::Managed;
    }
    if let Some(home) = home {
        let user_managed_roots = [
            home.join(".local/bin"),
            home.join(".local/share/pnpm"),
            home.join(".local/share/uv"),
            home.join(".bun/bin"),
            home.join(".cargo/bin"),
            home.join(".nvm/versions/node"),
            home.join(".volta/bin"),
            home.join(".fnm"),
            home.join(".asdf"),
            home.join(".mise"),
            home.join(".pyenv"),
            home.join(".rbenv"),
            home.join(".rustup/toolchains"),
        ];
        if user_managed_roots.iter().any(|root| {
            let root = root.canonicalize().unwrap_or_else(|_| root.clone());
            resolved.starts_with(root)
        }) {
            return ExecutionTrust::UserManaged;
        }
    }
    ExecutionTrust::Unverified
}

pub fn find_all_in_path_for_names(names: &[&str]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    env::var_os("PATH")
        .into_iter()
        .flat_map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .flat_map(|directory| names.iter().map(move |name| directory.join(name)))
        .filter_map(|path| {
            if path.is_file() && seen.insert(path.clone()) {
                Some(path)
            } else {
                None
            }
        })
        .collect()
}

pub fn readable_path(path: &Path) -> String {
    let value = path.to_string_lossy().into_owned();
    if let Some(home) = dirs::home_dir() {
        let home = home.to_string_lossy();
        if value.starts_with(home.as_ref()) {
            return value.replacen(home.as_ref(), "~", 1);
        }
    }
    value
}

pub(crate) fn redact_and_truncate(value: &str) -> String {
    let home_redacted = if let Some(home) = dirs::home_dir() {
        value.replace(home.to_string_lossy().as_ref(), "~")
    } else {
        value.to_string()
    };
    let redacted = redact_sensitive_fragments(home_redacted);
    if redacted.len() <= OUTPUT_LIMIT {
        return redacted.trim().to_string();
    }
    let boundary = redacted
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= OUTPUT_LIMIT)
        .last()
        .unwrap_or(OUTPUT_LIMIT);
    format!("{}\n…输出已截断", redacted[..boundary].trim())
}

fn redact_sensitive_fragments(mut value: String) -> String {
    let mut cursor = 0;
    while let Some(offset) = value[cursor..].find("://") {
        let authority_start = cursor + offset + 3;
        let authority_end = value[authority_start..]
            .char_indices()
            .find(|(_, character)| {
                character.is_whitespace() || matches!(character, '/' | '?' | '#')
            })
            .map(|(index, _)| authority_start + index)
            .unwrap_or(value.len());
        let authority = &value[authority_start..authority_end];
        if let Some(at) = authority.rfind('@') {
            value.replace_range(authority_start..authority_start + at, "[REDACTED]");
            cursor = authority_start + "[REDACTED]@".len();
        } else {
            cursor = authority_end;
        }
    }
    for key in ["access_token=", "token=", "password=", "passwd="] {
        let mut cursor = 0;
        loop {
            let lower = value.to_ascii_lowercase();
            let Some(offset) = lower[cursor..].find(key) else {
                break;
            };
            let secret_start = cursor + offset + key.len();
            if value[secret_start..].starts_with("[REDACTED]") {
                cursor = secret_start + "[REDACTED]".len();
                continue;
            }
            let secret_end = value[secret_start..]
                .char_indices()
                .find(|(_, character)| {
                    character.is_whitespace() || matches!(character, '&' | ';' | ',' | '\'' | '"')
                })
                .map(|(index, _)| secret_start + index)
                .unwrap_or(value.len());
            value.replace_range(secret_start..secret_end, "[REDACTED]");
            cursor = secret_start + "[REDACTED]".len();
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn keeps_arguments_out_of_a_shell() {
        let output = CommandRunner {
            timeout: Duration::from_secs(2),
        }
        .run_cancellable(
            Path::new("/bin/echo"),
            &["$(touch /tmp/should-not-exist)"],
            &AtomicBool::new(false),
        );
        assert!(output.success);
        assert!(output.stdout.contains("$(touch"));
    }

    #[cfg(unix)]
    #[test]
    fn prepends_the_executable_directory_for_env_shebangs() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().unwrap();
        let node = directory.path().join("node");
        let npm = directory.path().join("npm");
        std::os::unix::fs::symlink("/bin/echo", &node).unwrap();
        fs::write(&npm, "#!/usr/bin/env node\n").unwrap();
        fs::set_permissions(&npm, fs::Permissions::from_mode(0o755)).unwrap();

        let output = CommandRunner {
            timeout: Duration::from_secs(2),
        }
        .run_cancellable(&npm, &["--version"], &AtomicBool::new(false));

        assert!(output.success, "{}", output.stderr);
        assert!(output.stdout.contains("npm"));
        assert!(output.stdout.contains("--version"));
    }

    #[test]
    fn redacts_url_credentials_and_common_secret_parameters() {
        let output = redact_and_truncate(
            "fetch https://user:pass@example.com/pkg?token=secret&name=x password=hunter2",
        );
        assert_eq!(
            output,
            "fetch https://[REDACTED]@example.com/pkg?token=[REDACTED]&name=x password=[REDACTED]"
        );
        assert!(!output.contains("hunter2"));
        assert!(!output.contains("user:pass"));
    }

    #[test]
    fn classifies_known_and_unverified_executable_paths() {
        assert_eq!(
            execution_trust(
                Path::new("/opt/homebrew/bin/brew"),
                &["/opt/homebrew/bin/brew"]
            ),
            ExecutionTrust::Managed
        );
        assert_eq!(
            execution_trust(Path::new("/tmp/unexpected/npm"), &[]),
            ExecutionTrust::Unverified
        );
        assert!(!can_execute(ExecutionTrust::Unverified));
        assert!(can_execute(ExecutionTrust::UserManaged));
    }

    #[test]
    fn recognizes_a_tool_inside_an_explicit_user_managed_root() {
        let home = tempdir().unwrap();
        let executable = home.path().join(".local/bin/uv");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, "").unwrap();

        assert_eq!(
            execution_trust_with_home(&executable, Some(home.path())),
            ExecutionTrust::UserManaged
        );
    }

    #[cfg(unix)]
    #[test]
    fn does_not_trust_a_known_looking_symlink_to_an_unverified_target() {
        use std::os::unix::fs::symlink;

        let home = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let target = outside.path().join("unverified-tool");
        let link = home.path().join(".local/bin/trusted-looking-tool");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        fs::write(&target, "").unwrap();
        symlink(&target, &link).unwrap();

        assert_eq!(
            execution_trust_with_home(&link, Some(home.path())),
            ExecutionTrust::Unverified
        );
    }
}
