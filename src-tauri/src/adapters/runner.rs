use std::{
    collections::HashSet,
    env,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use wait_timeout::ChildExt;

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

    pub fn run(&self, executable: &Path, args: &[&str]) -> CommandOutput {
        let mut child = match Command::new(executable)
            .args(args)
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
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
        let (success, exit_code, fallback_error) = match child.wait_timeout(self.timeout) {
            Ok(Some(status)) => (status.success(), status.code(), None),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                (
                    false,
                    None,
                    Some(format!(
                        "命令执行超过 {} 秒，已终止",
                        self.timeout.as_secs()
                    )),
                )
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                (false, None, Some(error.to_string()))
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

pub fn find_executable(names: &[&str], common_paths: &[&str]) -> Option<PathBuf> {
    for name in names {
        if let Some(path) = find_all_in_path(name).into_iter().next() {
            return Some(path);
        }
    }
    common_paths
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

pub fn find_all_in_path(command: &str) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    env::var_os("PATH")
        .into_iter()
        .flat_map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .filter_map(|directory| {
            let path = directory.join(command);
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

fn redact_and_truncate(value: &str) -> String {
    let redacted = if let Some(home) = dirs::home_dir() {
        value.replace(home.to_string_lossy().as_ref(), "~")
    } else {
        value.to_string()
    };
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

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn discovers_executable_from_path() {
        let directory = tempdir().unwrap();
        let executable = directory.path().join("devpkg-test-bin");
        fs::write(&executable, "").unwrap();
        let previous = env::var_os("PATH");
        env::set_var("PATH", directory.path());
        assert_eq!(find_executable(&["devpkg-test-bin"], &[]), Some(executable));
        match previous {
            Some(value) => env::set_var("PATH", value),
            None => env::remove_var("PATH"),
        }
    }

    #[test]
    fn keeps_arguments_out_of_a_shell() {
        let output = CommandRunner {
            timeout: Duration::from_secs(2),
        }
        .run(Path::new("/bin/echo"), &["$(touch /tmp/should-not-exist)"]);
        assert!(output.success);
        assert!(output.stdout.contains("$(touch"));
    }
}
