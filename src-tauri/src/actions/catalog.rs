use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime},
};

use crate::{
    adapters::runner::{redact_and_truncate, CommandRunner},
    error::AppError,
    models::{
        CatalogSearchBlockerCode, CatalogSearchResponse, CatalogSearchResult, CatalogSearchStatus,
        ExecutionTrust, ManagerStatus, NetworkPolicy, PackageManagerId,
    },
    storage::Storage,
};

use super::{
    is_valid_formula_name, is_valid_registry_package_name, validate_homebrew_executable,
    validate_npm_executable, validate_pnpm_executable,
};

const CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const CACHE_LIMIT: usize = 50;
const RESULT_LIMIT: usize = 20;

#[derive(Debug, Clone)]
struct CatalogEntry {
    name: String,
    description: Option<String>,
    version: Option<String>,
}

#[derive(Debug, Clone)]
struct CachedSearch {
    cached_at: SystemTime,
    results: Vec<CatalogEntry>,
}

#[derive(Clone, Default)]
pub struct CatalogRegistry {
    active: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    cache: Arc<Mutex<HashMap<(PackageManagerId, String), CachedSearch>>>,
}

impl CatalogRegistry {
    pub fn begin(&self, search_id: &str) -> Result<Arc<AtomicBool>, AppError> {
        let mut active = self
            .active
            .lock()
            .map_err(|error| AppError::Command(error.to_string()))?;
        if active.contains_key(search_id) {
            return Err(AppError::Command(
                "CATALOG_SEARCH_ALREADY_RUNNING：搜索标识已在使用".into(),
            ));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        active.insert(search_id.into(), cancelled.clone());
        Ok(cancelled)
    }

    pub fn finish(&self, search_id: &str) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(search_id);
        }
    }

    pub fn cancel(&self, search_id: &str) {
        if let Ok(active) = self.active.lock() {
            if let Some(cancelled) = active.get(search_id) {
                cancelled.store(true, Ordering::SeqCst);
            }
        }
    }

    fn cached(&self, manager_id: PackageManagerId, query: &str) -> Option<Vec<CatalogEntry>> {
        let mut cache = self.cache.lock().ok()?;
        cache.retain(|_, entry| {
            entry
                .cached_at
                .elapsed()
                .map(|elapsed| elapsed <= CACHE_TTL)
                .unwrap_or(false)
        });
        cache
            .get(&(manager_id, query.into()))
            .map(|entry| entry.results.clone())
    }

    fn cache(&self, manager_id: PackageManagerId, query: String, results: Vec<CatalogEntry>) {
        let Ok(mut cache) = self.cache.lock() else {
            return;
        };
        cache.retain(|_, entry| {
            entry
                .cached_at
                .elapsed()
                .map(|elapsed| elapsed <= CACHE_TTL)
                .unwrap_or(false)
        });
        while cache.len() >= CACHE_LIMIT && !cache.contains_key(&(manager_id, query.clone())) {
            let Some(oldest_key) = cache
                .iter()
                .min_by_key(|(_, entry)| entry.cached_at)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            cache.remove(&oldest_key);
        }
        cache.insert(
            (manager_id, query),
            CachedSearch {
                cached_at: SystemTime::now(),
                results,
            },
        );
    }
}

pub fn search(
    storage: &Storage,
    registry: &CatalogRegistry,
    search_id: String,
    manager_id: PackageManagerId,
    query: String,
    cancelled: &AtomicBool,
) -> Result<CatalogSearchResponse, AppError> {
    let query = query.trim().to_string();
    if storage.scan_settings()?.network_policy != NetworkPolicy::Registry {
        return Ok(blocked(
            search_id,
            manager_id,
            query,
            CatalogSearchStatus::Offline,
            CatalogSearchBlockerCode::NetworkPolicyOffline,
            "当前联网策略为离线。请在项目页将联网策略切换为“允许访问软件源”后再搜索。",
        ));
    }
    if !is_valid_catalog_query(&query) {
        return Ok(blocked(
            search_id,
            manager_id,
            query,
            CatalogSearchStatus::InvalidQuery,
            CatalogSearchBlockerCode::InvalidQuery,
            "搜索词必须为 2 至 64 个安全字符，且不能是选项、URL、Git 地址或本地路径。",
        ));
    }
    if !matches!(
        manager_id,
        PackageManagerId::Homebrew | PackageManagerId::Npm | PackageManagerId::Pnpm
    ) {
        return Ok(blocked(
            search_id,
            manager_id,
            query,
            CatalogSearchStatus::ManagerUnavailable,
            CatalogSearchBlockerCode::UnsupportedManager,
            "当前目录搜索仅支持 Homebrew、npm 与 pnpm。",
        ));
    }
    let Some(snapshot) = storage.latest_snapshot()? else {
        return Ok(blocked(
            search_id,
            manager_id,
            query,
            CatalogSearchStatus::ManagerUnavailable,
            CatalogSearchBlockerCode::MissingScan,
            "请先完成一次环境扫描，再使用软件包目录搜索。",
        ));
    };
    let Some(manager) = snapshot
        .managers
        .iter()
        .find(|manager| manager.id == manager_id)
    else {
        return Ok(blocked(
            search_id,
            manager_id,
            query,
            CatalogSearchStatus::ManagerUnavailable,
            CatalogSearchBlockerCode::ManagerUnavailable,
            "扫描结果中未发现此包管理器。",
        ));
    };
    if !matches!(manager.status, ManagerStatus::Available) {
        return Ok(blocked(
            search_id,
            manager_id,
            query,
            CatalogSearchStatus::ManagerUnavailable,
            CatalogSearchBlockerCode::ManagerUnavailable,
            "包管理器当前不可用，无法执行目录搜索。",
        ));
    }
    let trusted = match manager_id {
        PackageManagerId::Homebrew => manager.execution_trust == ExecutionTrust::Managed,
        PackageManagerId::Npm | PackageManagerId::Pnpm => matches!(
            manager.execution_trust,
            ExecutionTrust::Managed | ExecutionTrust::UserManaged
        ),
        _ => false,
    };
    let executable = manager.executable_path.as_deref().map(PathBuf::from);
    if !trusted || executable.is_none() {
        return Ok(blocked(
            search_id,
            manager_id,
            query,
            CatalogSearchStatus::UntrustedExecutable,
            CatalogSearchBlockerCode::UntrustedExecutable,
            "包管理器可执行文件不在允许的可信目录中，目录搜索已阻止。",
        ));
    }
    let executable = executable.expect("checked above");
    let validation = match manager_id {
        PackageManagerId::Homebrew => validate_homebrew_executable(&executable),
        PackageManagerId::Npm => validate_npm_executable(&executable),
        PackageManagerId::Pnpm => validate_pnpm_executable(&executable),
        _ => unreachable!(),
    };
    if validation.is_err() {
        return Ok(blocked(
            search_id,
            manager_id,
            query,
            CatalogSearchStatus::UntrustedExecutable,
            CatalogSearchBlockerCode::UntrustedExecutable,
            "包管理器路径在本次搜索前未通过复验，目录搜索已阻止。",
        ));
    }
    if cancelled.load(Ordering::SeqCst) {
        return Ok(cancelled_response(search_id, manager_id, query));
    }

    let entries = if let Some(entries) = registry.cached(manager_id, &query) {
        entries
    } else {
        let output = CommandRunner::default().run_cancellable(
            &executable,
            &catalog_args(manager_id, &query),
            cancelled,
        );
        if cancelled.load(Ordering::SeqCst) {
            return Ok(cancelled_response(search_id, manager_id, query));
        }
        if !output.success {
            return Ok(blocked(
                search_id,
                manager_id,
                query,
                CatalogSearchStatus::Error,
                CatalogSearchBlockerCode::SearchFailed,
                &format!(
                    "目录搜索失败：{}",
                    redact_and_truncate(&output.combined_output())
                ),
            ));
        }
        let entries = parse_results(manager_id, &output.stdout);
        registry.cache(manager_id, query.clone(), entries.clone());
        entries
    };
    let results = entries
        .into_iter()
        .map(|entry| {
            let installed = snapshot
                .packages
                .iter()
                .find(|package| package.manager_id == manager_id && package.name == entry.name);
            CatalogSearchResult {
                manager_id,
                name: entry.name,
                description: entry.description,
                version: entry.version,
                installed: installed.is_some(),
                installed_version: installed.map(|package| package.version.clone()),
            }
        })
        .collect();
    Ok(CatalogSearchResponse {
        search_id,
        manager_id,
        query,
        status: CatalogSearchStatus::Ready,
        blocker_code: None,
        message: "目录搜索仅用于选择安装目标；不会生成计划或执行安装。".into(),
        results,
    })
}

fn catalog_args(manager_id: PackageManagerId, query: &str) -> Vec<&str> {
    match manager_id {
        PackageManagerId::Homebrew => vec!["search", "--formula", query],
        PackageManagerId::Npm => vec!["search", "--json", "--searchlimit=20", query],
        PackageManagerId::Pnpm => vec!["search", "--json", "--search-limit=20", query],
        _ => Vec::new(),
    }
}

fn parse_results(manager_id: PackageManagerId, output: &str) -> Vec<CatalogEntry> {
    let entries = match manager_id {
        PackageManagerId::Homebrew => parse_homebrew_results(output),
        PackageManagerId::Npm | PackageManagerId::Pnpm => parse_registry_results(output),
        _ => Vec::new(),
    };
    entries.into_iter().take(RESULT_LIMIT).collect()
}

fn parse_homebrew_results(output: &str) -> Vec<CatalogEntry> {
    output
        .split_whitespace()
        .filter(|name| is_valid_formula_name(name))
        .map(|name| CatalogEntry {
            name: name.into(),
            description: None,
            version: None,
        })
        .collect()
}

fn parse_registry_results(output: &str) -> Vec<CatalogEntry> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(output) else {
        return Vec::new();
    };
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let name = item.get("name")?.as_str()?;
            is_valid_registry_package_name(name).then(|| CatalogEntry {
                name: name.into(),
                description: item
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(redact_and_truncate),
                version: item
                    .get("version")
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_string),
            })
        })
        .collect()
}

fn is_valid_catalog_query(value: &str) -> bool {
    if !(2..=64).contains(&value.len())
        || value.starts_with('-')
        || value.starts_with('.')
        || value.contains("..")
        || value.contains("://")
        || value.starts_with("git+")
        || value.starts_with("git@")
        || value.contains('\\')
        || value.starts_with('/')
        || value.starts_with("~/")
    {
        return false;
    }
    value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'@' | b'/' | b'-' | b'_' | b'.')
    })
}

fn blocked(
    search_id: String,
    manager_id: PackageManagerId,
    query: String,
    status: CatalogSearchStatus,
    blocker_code: CatalogSearchBlockerCode,
    message: &str,
) -> CatalogSearchResponse {
    CatalogSearchResponse {
        search_id,
        manager_id,
        query,
        status,
        blocker_code: Some(blocker_code),
        message: message.into(),
        results: Vec::new(),
    }
}

fn cancelled_response(
    search_id: String,
    manager_id: PackageManagerId,
    query: String,
) -> CatalogSearchResponse {
    blocked(
        search_id,
        manager_id,
        query,
        CatalogSearchStatus::Cancelled,
        CatalogSearchBlockerCode::Cancelled,
        "目录搜索已取消，未生成安装计划。",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_commands_are_fixed_and_limited() {
        assert_eq!(
            catalog_args(PackageManagerId::Homebrew, "ripgrep"),
            ["search", "--formula", "ripgrep"]
        );
        assert_eq!(
            catalog_args(PackageManagerId::Npm, "eslint"),
            ["search", "--json", "--searchlimit=20", "eslint"]
        );
        assert_eq!(
            catalog_args(PackageManagerId::Pnpm, "eslint"),
            ["search", "--json", "--search-limit=20", "eslint"]
        );
    }

    #[test]
    fn rejects_options_urls_git_and_local_paths() {
        for value in [
            "-registry",
            "https://registry.npmjs.org",
            "git+https://github.com/a/b",
            "git@github.com:a/b",
            "../tool",
            "~/tool",
            "/tmp/tool",
            "a\\b",
        ] {
            assert!(!is_valid_catalog_query(value), "{value}");
        }
        assert!(is_valid_catalog_query("@scope/tool"));
        assert!(is_valid_catalog_query("ripgrep"));
    }

    #[test]
    fn registry_results_drop_unsafe_package_names() {
        let results = parse_registry_results(
            r#"[
          {"name":"eslint","description":"linter","version":"9.0.0"},
          {"name":"git+https://example.invalid/tool","version":"1.0.0"},
          {"name":"@scope/tool","version":"2.0.0"}
        ]"#,
        );
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "eslint");
        assert_eq!(results[1].name, "@scope/tool");
    }

    #[test]
    fn homebrew_results_only_accept_formula_names() {
        let results = parse_homebrew_results("ripgrep jq ../../unsafe user/tap/formula");
        assert_eq!(
            results
                .into_iter()
                .map(|entry| entry.name)
                .collect::<Vec<_>>(),
            ["ripgrep", "jq", "user/tap/formula"]
        );
    }
}
