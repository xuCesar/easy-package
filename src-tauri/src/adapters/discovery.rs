use std::{
    collections::HashSet,
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};

const MANAGED_BIN_DIRECTORIES: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];

pub(super) fn find_executable(names: &[&str], common_paths: &[&str]) -> Option<PathBuf> {
    let managed_bin_directories = MANAGED_BIN_DIRECTORIES.map(PathBuf::from);
    find_executable_with_environment(
        names,
        common_paths,
        env::var_os("PATH").as_deref(),
        dirs::home_dir().as_deref(),
        &managed_bin_directories,
    )
}

fn find_executable_with_environment(
    names: &[&str],
    common_paths: &[&str],
    path: Option<&OsStr>,
    home: Option<&Path>,
    managed_bin_directories: &[PathBuf],
) -> Option<PathBuf> {
    common_paths
        .iter()
        .map(PathBuf::from)
        .chain(find_all_in_search_path(names, path))
        .chain(known_executable_paths(names, home, managed_bin_directories))
        .find(|candidate| candidate.is_file())
}

fn known_executable_paths(
    names: &[&str],
    home: Option<&Path>,
    managed_bin_directories: &[PathBuf],
) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(home) = home {
        if names.iter().any(|name| is_node_manager(name)) {
            directories.extend(nvm_default_bin(home));
        }
        directories.extend([
            home.join(".local/bin"),
            home.join(".local/share/pnpm"),
            home.join(".bun/bin"),
            home.join(".cargo/bin"),
            home.join(".volta/bin"),
            home.join(".asdf/shims"),
            home.join(".mise/shims"),
        ]);
    }
    directories.extend(managed_bin_directories.iter().cloned());

    directories
        .into_iter()
        .flat_map(|directory| names.iter().map(move |name| directory.join(name)))
        .collect()
}

pub(super) fn command_search_path(executable: &Path) -> Option<OsString> {
    command_search_path_with_environment(
        executable,
        env::var_os("PATH").as_deref(),
        dirs::home_dir().as_deref(),
        &MANAGED_BIN_DIRECTORIES.map(PathBuf::from),
    )
}

fn command_search_path_with_environment(
    executable: &Path,
    path: Option<&OsStr>,
    home: Option<&Path>,
    managed_bin_directories: &[PathBuf],
) -> Option<OsString> {
    let mut seen = HashSet::new();
    let mut directories = executable
        .parent()
        .into_iter()
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    if executable
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(is_node_manager)
    {
        directories.extend(home.and_then(nvm_default_bin));
        directories.extend(managed_bin_directories.iter().cloned());
    }
    directories.extend(path.into_iter().flat_map(env::split_paths));
    directories.retain(|directory| seen.insert(directory.clone()));
    env::join_paths(directories).ok()
}

fn is_node_manager(name: &str) -> bool {
    matches!(name, "npm" | "npx" | "pnpm" | "yarn" | "corepack")
}

/// GUI 应用不会加载 shell 配置，因此只读取 nvm 自己的别名文件解析默认版本。
fn nvm_default_bin(home: &Path) -> Option<PathBuf> {
    let nvm_root = home.join(".nvm");
    let default_alias = fs::read_to_string(nvm_root.join("alias/default")).ok()?;
    resolve_nvm_alias(&nvm_root, default_alias.trim(), 0)
        .map(|version| nvm_root.join("versions/node").join(version).join("bin"))
}

fn resolve_nvm_alias(nvm_root: &Path, alias: &str, depth: usize) -> Option<String> {
    if depth >= 8 || alias.is_empty() || alias == "system" {
        return None;
    }
    if matches!(alias, "node" | "stable") {
        return latest_nvm_version(nvm_root, None);
    }
    if alias == "lts/*" {
        let versions = fs::read_dir(nvm_root.join("alias/lts"))
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| fs::read_to_string(entry.path()).ok())
            .filter_map(|value| latest_nvm_version(nvm_root, Some(value.trim())))
            .collect::<Vec<_>>();
        return versions
            .into_iter()
            .max_by_key(|version| parse_node_version(version));
    }
    if let Some(lts_name) = alias.strip_prefix("lts/") {
        if is_safe_alias_name(lts_name) {
            let value = fs::read_to_string(nvm_root.join("alias/lts").join(lts_name)).ok()?;
            return resolve_nvm_alias(nvm_root, value.trim(), depth + 1);
        }
        return None;
    }
    if is_safe_alias_name(alias) {
        if let Ok(value) = fs::read_to_string(nvm_root.join("alias").join(alias)) {
            return resolve_nvm_alias(nvm_root, value.trim(), depth + 1);
        }
    }
    latest_nvm_version(nvm_root, Some(alias))
}

fn is_safe_alias_name(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
}

fn latest_nvm_version(nvm_root: &Path, prefix: Option<&str>) -> Option<String> {
    let normalized_prefix = prefix.map(|value| value.trim().trim_start_matches('v'));
    fs::read_dir(nvm_root.join("versions/node"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|version| {
            normalized_prefix.is_none_or(|prefix| {
                let version = version.trim_start_matches('v');
                version == prefix || version.starts_with(&format!("{prefix}."))
            })
        })
        .filter(|version| parse_node_version(version).is_some())
        .max_by_key(|version| parse_node_version(version))
}

fn parse_node_version(value: &str) -> Option<(u64, u64, u64)> {
    let mut parts = value.trim().trim_start_matches('v').split('.');
    let version = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(version)
}

fn find_all_in_search_path(names: &[&str], path: Option<&OsStr>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    path.into_iter()
        .flat_map(env::split_paths)
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn discovers_executable_from_path() {
        let directory = tempdir().unwrap();
        let executable = directory.path().join("devpkg-test-bin");
        fs::write(&executable, "").unwrap();

        assert_eq!(
            find_executable_with_environment(
                &["devpkg-test-bin"],
                &[],
                Some(directory.path().as_os_str()),
                None,
                &[],
            ),
            Some(executable)
        );
    }

    #[test]
    fn discovers_npm_from_the_nvm_default_without_shell_path() {
        let home = tempdir().unwrap();
        let nvm = home.path().join(".nvm");
        fs::create_dir_all(nvm.join("alias/lts")).unwrap();
        fs::write(nvm.join("alias/default"), "lts/*\n").unwrap();
        fs::write(nvm.join("alias/lts/iron"), "v20.20.2\n").unwrap();
        fs::write(nvm.join("alias/lts/krypton"), "v24.15.0\n").unwrap();
        for version in ["v20.20.2", "v24.15.0"] {
            let bin = nvm.join("versions/node").join(version).join("bin");
            fs::create_dir_all(&bin).unwrap();
            fs::write(bin.join("npm"), "").unwrap();
        }

        assert_eq!(
            find_executable_with_environment(
                &["npm"],
                &[],
                Some(OsStr::new("/usr/bin:/bin")),
                Some(home.path()),
                &[],
            ),
            Some(nvm.join("versions/node/v24.15.0/bin/npm"))
        );
    }

    #[test]
    fn discovers_pnpm_from_a_managed_bin_without_shell_path() {
        let managed_bin = tempdir().unwrap();
        let pnpm = managed_bin.path().join("pnpm");
        fs::write(&pnpm, "").unwrap();

        assert_eq!(
            find_executable_with_environment(
                &["pnpm"],
                &[],
                Some(OsStr::new("/usr/bin:/bin")),
                None,
                &[managed_bin.path().to_path_buf()],
            ),
            Some(pnpm)
        );
    }

    #[test]
    fn rejects_nvm_aliases_that_could_escape_the_alias_directory() {
        let home = tempdir().unwrap();
        let nvm = home.path().join(".nvm");
        fs::create_dir_all(nvm.join("alias")).unwrap();
        fs::write(nvm.join("alias/default"), "../../outside\n").unwrap();

        assert_eq!(nvm_default_bin(home.path()), None);
    }
}
