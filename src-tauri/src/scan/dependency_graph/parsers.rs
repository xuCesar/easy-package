//! npm / pnpm / Cargo 锁文件的只读解析器与解析辅助。
use super::*;
pub(super) fn parse_npm_lock(
    path: &Path,
    project_directory: &Path,
    root_id: &str,
    cancelled: &AtomicBool,
) -> Result<ParsedGraph, AppError> {
    let source_name = "package-lock.json";
    let source = match read_lock_file(path, source_name)? {
        LockSource::Content(source) => source,
        LockSource::TooLarge(size) => {
            return Ok(ParsedGraph::empty(
                source_name,
                DependencyGraphCompleteness::Partial,
                format!("{source_name} 大小为 {size} 字节，超过 25 MB 解析预算。"),
            ))
        }
    };
    let digest = stable_digest(source.as_bytes());
    let Ok(value) = serde_json::from_str::<JsonValue>(&source) else {
        return Ok(ParsedGraph::empty(
            source_name,
            DependencyGraphCompleteness::Invalid,
            format!("{source_name} 不是有效 JSON。"),
        ));
    };
    let Some(packages) = value.get("packages").and_then(JsonValue::as_object) else {
        return Ok(ParsedGraph::empty(
            source_name,
            DependencyGraphCompleteness::Invalid,
            format!("{source_name} 缺少 packages 对象。"),
        ));
    };
    let lock_root = path.parent().unwrap_or(project_directory);
    let importer = project_directory
        .strip_prefix(lock_root)
        .ok()
        .map(normalized_relative_path)
        .filter(|value| !value.is_empty() && packages.contains_key(value))
        .unwrap_or_default();
    let mut nodes = BTreeMap::new();
    let mut key_to_id = HashMap::new();
    let mut warnings = Vec::new();
    let mut completeness = DependencyGraphCompleteness::Complete;
    for (key, package) in packages {
        if cancelled.load(Ordering::SeqCst) {
            return Err(AppError::ScanCancelled);
        }
        if key == &importer || key.is_empty() {
            continue;
        }
        let Some(name) = package
            .get("name")
            .and_then(JsonValue::as_str)
            .map(str::to_string)
            .or_else(|| npm_name_from_key(key))
        else {
            continue;
        };
        let linked_key = package
            .get("link")
            .and_then(JsonValue::as_bool)
            .filter(|linked| *linked)
            .and_then(|_| package.get("resolved").and_then(JsonValue::as_str))
            .map(normalize_lock_key);
        let version = package
            .get("version")
            .and_then(JsonValue::as_str)
            .or_else(|| {
                linked_key
                    .as_ref()
                    .and_then(|target| packages.get(target))
                    .and_then(|target| target.get("version"))
                    .and_then(JsonValue::as_str)
            })
            .unwrap_or("local");
        let id = format!("npm:{key}");
        key_to_id.insert(key.clone(), id.clone());
        if let Some(linked_key) = linked_key {
            key_to_id.insert(linked_key, id.clone());
        }
        nodes.insert(
            id.clone(),
            package_node(&id, "JavaScript", &name, version, false),
        );
        if nodes.len() >= MAX_GRAPH_NODES {
            completeness = DependencyGraphCompleteness::Partial;
            warnings.push(format!("依赖节点超过 {MAX_GRAPH_NODES} 个，结果已截断。"));
            break;
        }
    }

    let mut edges = Vec::new();
    for (key, package) in packages {
        if cancelled.load(Ordering::SeqCst) {
            return Err(AppError::ScanCancelled);
        }
        let from = if key == &importer || (importer.is_empty() && key.is_empty()) {
            Some(root_id.to_string())
        } else {
            key_to_id.get(key).cloned()
        };
        let Some(from) = from else {
            continue;
        };
        for (scope, dependency_type) in [
            ("dependencies", "runtime"),
            ("devDependencies", "development"),
            ("optionalDependencies", "optional"),
            ("peerDependencies", "peer"),
        ] {
            let dependencies = package.get(scope).and_then(JsonValue::as_object);
            for name in dependencies.into_iter().flatten().map(|(name, _)| name) {
                let target_key = resolve_npm_target(packages, key, name);
                let target_id = target_key
                    .as_ref()
                    .and_then(|target| key_to_id.get(target))
                    .cloned();
                let Some(target_id) = target_id else {
                    completeness =
                        merge_completeness(completeness, DependencyGraphCompleteness::Partial);
                    push_unique(
                        &mut warnings,
                        format!("无法在 {source_name} 中解析依赖 {name} 的安装位置。"),
                    );
                    continue;
                };
                if from == root_id {
                    mark_direct(&mut nodes, &target_id, dependency_type);
                }
                push_edge(
                    &mut edges,
                    &from,
                    &target_id,
                    dependency_type,
                    &mut completeness,
                    &mut warnings,
                );
            }
        }
    }
    let raw_node_count = nodes.len();
    prune_unreachable(root_id, &mut nodes, &mut edges);
    Ok(ParsedGraph {
        nodes,
        edges,
        raw_node_count,
        completeness,
        warnings,
        source: source_name.into(),
        digest,
    })
}

pub(super) fn parse_pnpm_lock(
    path: &Path,
    project_directory: &Path,
    root_id: &str,
    cancelled: &AtomicBool,
) -> Result<ParsedGraph, AppError> {
    let source_name = "pnpm-lock.yaml";
    let source = match read_lock_file(path, source_name)? {
        LockSource::Content(source) => source,
        LockSource::TooLarge(size) => {
            return Ok(ParsedGraph::empty(
                source_name,
                DependencyGraphCompleteness::Partial,
                format!("{source_name} 大小为 {size} 字节，超过 25 MB 解析预算。"),
            ))
        }
    };
    let digest = stable_digest(source.as_bytes());
    let Ok(value) = serde_yaml::from_str::<YamlValue>(&source) else {
        return Ok(ParsedGraph::empty(
            source_name,
            DependencyGraphCompleteness::Invalid,
            format!("{source_name} 不是有效 YAML。"),
        ));
    };
    let Some(importers) = value.get("importers").and_then(YamlValue::as_mapping) else {
        return Ok(ParsedGraph::empty(
            source_name,
            DependencyGraphCompleteness::Invalid,
            format!("{source_name} 缺少 importers。"),
        ));
    };
    let lock_root = path.parent().unwrap_or(project_directory);
    let importer_key = project_directory
        .strip_prefix(lock_root)
        .ok()
        .map(normalized_relative_path)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| ".".into());
    let importer = importers
        .get(YamlValue::String(importer_key.clone()))
        .or_else(|| importers.get(YamlValue::String(".".into())));
    let Some(importer) = importer else {
        return Ok(ParsedGraph::empty(
            source_name,
            DependencyGraphCompleteness::Partial,
            format!("{source_name} 中没有项目 importer：{importer_key}。"),
        ));
    };
    let packages = value.get("packages").and_then(YamlValue::as_mapping);
    let snapshots = value.get("snapshots").and_then(YamlValue::as_mapping);
    let mut keys = BTreeSet::new();
    for mapping in [packages, snapshots].into_iter().flatten() {
        keys.extend(
            mapping
                .keys()
                .filter_map(YamlValue::as_str)
                .map(str::to_string),
        );
    }
    let mut nodes = BTreeMap::new();
    let mut key_to_id = HashMap::new();
    let mut warnings = Vec::new();
    let mut completeness = DependencyGraphCompleteness::Complete;
    for key in &keys {
        if cancelled.load(Ordering::SeqCst) {
            return Err(AppError::ScanCancelled);
        }
        let Some((name, version)) = pnpm_package_identity(key) else {
            continue;
        };
        let id = format!("pnpm:{}", key.trim_start_matches('/'));
        key_to_id.insert(key.clone(), id.clone());
        nodes.insert(
            id.clone(),
            package_node(&id, "JavaScript", &name, &version, false),
        );
        if nodes.len() >= MAX_GRAPH_NODES {
            completeness = DependencyGraphCompleteness::Partial;
            warnings.push(format!("依赖节点超过 {MAX_GRAPH_NODES} 个，结果已截断。"));
            break;
        }
    }

    let mut edges = Vec::new();
    for (scope, dependency_type) in [
        ("dependencies", "runtime"),
        ("devDependencies", "development"),
        ("optionalDependencies", "optional"),
    ] {
        for (name, declaration) in yaml_dependency_entries(importer, scope) {
            let target = resolve_pnpm_reference(name, declaration, &keys);
            let target_id = match target {
                PnpmTarget::Package(key) => key_to_id.get(&key).cloned(),
                PnpmTarget::Local(reference) => {
                    let id = format!("pnpm-local:{name}:{reference}");
                    nodes
                        .entry(id.clone())
                        .or_insert_with(|| DependencyGraphNode {
                            id: id.clone(),
                            ecosystem: "JavaScript".into(),
                            name: name.into(),
                            version: reference,
                            kind: DependencyGraphNodeKind::Local,
                            direct: true,
                            scopes: vec![dependency_type.into()],
                            package_url: None,
                        });
                    Some(id)
                }
                PnpmTarget::Missing => None,
            };
            let Some(target_id) = target_id else {
                completeness =
                    merge_completeness(completeness, DependencyGraphCompleteness::Partial);
                push_unique(
                    &mut warnings,
                    format!("无法在 {source_name} 中解析 importer 依赖 {name}。"),
                );
                continue;
            };
            mark_direct(&mut nodes, &target_id, dependency_type);
            push_edge(
                &mut edges,
                root_id,
                &target_id,
                dependency_type,
                &mut completeness,
                &mut warnings,
            );
        }
    }

    if let Some(snapshots) = snapshots {
        for (key_value, snapshot) in snapshots {
            if cancelled.load(Ordering::SeqCst) {
                return Err(AppError::ScanCancelled);
            }
            let Some(key) = key_value.as_str() else {
                continue;
            };
            let Some(from) = key_to_id.get(key).cloned() else {
                continue;
            };
            for (scope, dependency_type) in [
                ("dependencies", "runtime"),
                ("optionalDependencies", "optional"),
            ] {
                for (name, declaration) in yaml_dependency_entries(snapshot, scope) {
                    let PnpmTarget::Package(target_key) =
                        resolve_pnpm_reference(name, declaration, &keys)
                    else {
                        continue;
                    };
                    let Some(target) = key_to_id.get(&target_key) else {
                        completeness =
                            merge_completeness(completeness, DependencyGraphCompleteness::Partial);
                        continue;
                    };
                    push_edge(
                        &mut edges,
                        &from,
                        target,
                        dependency_type,
                        &mut completeness,
                        &mut warnings,
                    );
                }
            }
        }
    }
    let raw_node_count = nodes.len();
    prune_unreachable(root_id, &mut nodes, &mut edges);
    Ok(ParsedGraph {
        nodes,
        edges,
        raw_node_count,
        completeness,
        warnings,
        source: source_name.into(),
        digest,
    })
}

pub(super) fn parse_cargo_lock(
    path: &Path,
    project: &ProjectMetadata,
    root_id: &str,
    cancelled: &AtomicBool,
) -> Result<ParsedGraph, AppError> {
    let source_name = "Cargo.lock";
    let source = match read_lock_file(path, source_name)? {
        LockSource::Content(source) => source,
        LockSource::TooLarge(size) => {
            return Ok(ParsedGraph::empty(
                source_name,
                DependencyGraphCompleteness::Partial,
                format!("{source_name} 大小为 {size} 字节，超过 25 MB 解析预算。"),
            ))
        }
    };
    let digest = stable_digest(source.as_bytes());
    let Ok(value) = source.parse::<TomlValue>() else {
        return Ok(ParsedGraph::empty(
            source_name,
            DependencyGraphCompleteness::Invalid,
            format!("{source_name} 不是有效 TOML。"),
        ));
    };
    let Some(packages) = value.get("package").and_then(TomlValue::as_array) else {
        return Ok(ParsedGraph::empty(
            source_name,
            DependencyGraphCompleteness::Invalid,
            format!("{source_name} 缺少 package 列表。"),
        ));
    };
    let mut nodes = BTreeMap::new();
    let mut identities = Vec::new();
    let mut warnings = Vec::new();
    let mut completeness = DependencyGraphCompleteness::Complete;
    for package in packages {
        if cancelled.load(Ordering::SeqCst) {
            return Err(AppError::ScanCancelled);
        }
        let Some(name) = package.get("name").and_then(TomlValue::as_str) else {
            continue;
        };
        let Some(version) = package.get("version").and_then(TomlValue::as_str) else {
            continue;
        };
        let source = package
            .get("source")
            .and_then(TomlValue::as_str)
            .unwrap_or("local");
        let id = format!("cargo:{name}@{version}:{source}");
        identities.push((
            name.to_string(),
            version.to_string(),
            source.to_string(),
            id.clone(),
        ));
        nodes.insert(id.clone(), package_node(&id, "Rust", name, version, false));
        if nodes.len() >= MAX_GRAPH_NODES {
            completeness = DependencyGraphCompleteness::Partial;
            warnings.push(format!("依赖节点超过 {MAX_GRAPH_NODES} 个，结果已截断。"));
            break;
        }
    }
    let root_package = packages.iter().find(|package| {
        package.get("name").and_then(TomlValue::as_str) == Some(project.name.as_str())
    });
    let mut edges = Vec::new();
    for package in packages {
        let Some(name) = package.get("name").and_then(TomlValue::as_str) else {
            continue;
        };
        let Some(version) = package.get("version").and_then(TomlValue::as_str) else {
            continue;
        };
        let source = package
            .get("source")
            .and_then(TomlValue::as_str)
            .unwrap_or("local");
        let package_id = format!("cargo:{name}@{version}:{source}");
        let from_root = root_package.is_some_and(|root| std::ptr::eq(root, package));
        let from = if from_root {
            root_id.to_string()
        } else {
            package_id
        };
        for dependency in package
            .get("dependencies")
            .and_then(TomlValue::as_array)
            .into_iter()
            .flatten()
            .filter_map(TomlValue::as_str)
        {
            let request = parse_cargo_dependency(dependency);
            let matches = identities
                .iter()
                .filter(|(candidate_name, candidate_version, candidate_source, _)| {
                    candidate_name == &request.name
                        && request
                            .version
                            .as_ref()
                            .is_none_or(|version| candidate_version == version)
                        && request
                            .source
                            .as_ref()
                            .is_none_or(|source| candidate_source == source)
                })
                .collect::<Vec<_>>();
            let Some((_, _, _, target)) = matches.first().copied() else {
                completeness =
                    merge_completeness(completeness, DependencyGraphCompleteness::Partial);
                push_unique(
                    &mut warnings,
                    format!("无法在 {source_name} 中解析 crate 依赖 {}。", request.name),
                );
                continue;
            };
            if matches.len() > 1 {
                completeness =
                    merge_completeness(completeness, DependencyGraphCompleteness::Partial);
                push_unique(
                    &mut warnings,
                    format!("crate {} 存在多个无法区分的锁定版本。", request.name),
                );
            }
            if from_root {
                mark_direct(&mut nodes, target, "runtime");
            }
            push_edge(
                &mut edges,
                &from,
                target,
                "runtime",
                &mut completeness,
                &mut warnings,
            );
        }
    }
    if root_package.is_none() {
        for dependency in project
            .dependencies
            .iter()
            .filter(|dependency| dependency.ecosystem == "Rust")
        {
            let matches = identities
                .iter()
                .filter(|(name, _, _, _)| name == &dependency.normalized_name)
                .collect::<Vec<_>>();
            if let Some((_, _, _, target)) = matches.first().copied() {
                mark_direct(&mut nodes, target, "runtime");
                push_edge(
                    &mut edges,
                    root_id,
                    target,
                    "runtime",
                    &mut completeness,
                    &mut warnings,
                );
            }
        }
    }
    let root_package_id = root_package.and_then(|package| {
        let name = package.get("name")?.as_str()?;
        let version = package.get("version")?.as_str()?;
        let source = package
            .get("source")
            .and_then(TomlValue::as_str)
            .unwrap_or("local");
        Some(format!("cargo:{name}@{version}:{source}"))
    });
    if let Some(root_package_id) = root_package_id {
        nodes.remove(&root_package_id);
        edges.retain(|edge| edge.to != root_package_id && edge.from != root_package_id);
    }
    let raw_node_count = nodes.len();
    prune_unreachable(root_id, &mut nodes, &mut edges);
    Ok(ParsedGraph {
        nodes,
        edges,
        raw_node_count,
        completeness,
        warnings,
        source: source_name.into(),
        digest,
    })
}

fn normalized_relative_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn normalize_lock_key(value: &str) -> String {
    value.trim_start_matches("./").replace('\\', "/")
}

fn npm_name_from_key(key: &str) -> Option<String> {
    let value = key
        .rsplit("/node_modules/")
        .next()
        .unwrap_or(key)
        .strip_prefix("node_modules/")
        .unwrap_or(key);
    (!value.is_empty()).then(|| value.to_string())
}

fn resolve_npm_target(
    packages: &serde_json::Map<String, JsonValue>,
    parent_key: &str,
    name: &str,
) -> Option<String> {
    let mut directory = PathBuf::from(parent_key);
    loop {
        let candidate = normalize_lock_key(
            directory
                .join("node_modules")
                .join(name)
                .to_string_lossy()
                .as_ref(),
        );
        if let Some(package) = packages.get(&candidate) {
            if package.get("link").and_then(JsonValue::as_bool) == Some(true) {
                if let Some(resolved) = package.get("resolved").and_then(JsonValue::as_str) {
                    let target = normalize_lock_key(resolved);
                    if packages.contains_key(&target) {
                        return Some(target);
                    }
                }
            }
            return Some(candidate);
        }
        if !directory.pop() {
            break;
        }
    }
    None
}

fn yaml_dependency_entries<'a>(value: &'a YamlValue, scope: &str) -> Vec<(&'a str, &'a YamlValue)> {
    value
        .get(scope)
        .and_then(YamlValue::as_mapping)
        .into_iter()
        .flatten()
        .filter_map(|(name, declaration)| Some((name.as_str()?, declaration)))
        .collect()
}

enum PnpmTarget {
    Package(String),
    Local(String),
    Missing,
}

fn resolve_pnpm_reference(
    name: &str,
    declaration: &YamlValue,
    keys: &BTreeSet<String>,
) -> PnpmTarget {
    let version = declaration
        .as_str()
        .or_else(|| declaration.get("version").and_then(YamlValue::as_str));
    let Some(version) = version else {
        return PnpmTarget::Missing;
    };
    if ["link:", "workspace:", "file:"]
        .iter()
        .any(|prefix| version.starts_with(prefix))
    {
        return PnpmTarget::Local(version.into());
    }
    let alias = version.strip_prefix("npm:");
    let candidate = if let Some(alias) = alias {
        alias.to_string()
    } else if version.starts_with('/') {
        version.trim_start_matches('/').to_string()
    } else if version.starts_with(name) && version[name.len()..].starts_with('@') {
        version.to_string()
    } else {
        format!("{name}@{version}")
    };
    let candidate_without_slash = candidate.trim_start_matches('/');
    keys.iter()
        .find(|key| key.trim_start_matches('/') == candidate_without_slash)
        .or_else(|| {
            keys.iter().find(|key| {
                key.trim_start_matches('/')
                    .starts_with(&format!("{candidate_without_slash}("))
            })
        })
        .cloned()
        .map(PnpmTarget::Package)
        .unwrap_or(PnpmTarget::Missing)
}

fn pnpm_package_identity(key: &str) -> Option<(String, String)> {
    let key = key.trim_start_matches('/');
    let without_peers = key.split('(').next().unwrap_or(key);
    let split = without_peers.rfind('@')?;
    if split == 0 {
        return None;
    }
    let name = &without_peers[..split];
    let version = &without_peers[split + 1..];
    (!name.is_empty() && !version.is_empty()).then(|| (name.into(), version.into()))
}

struct CargoDependencyRequest {
    name: String,
    version: Option<String>,
    source: Option<String>,
}

fn parse_cargo_dependency(value: &str) -> CargoDependencyRequest {
    let mut parts = value.split_whitespace();
    let name = parts.next().unwrap_or_default().to_string();
    let second = parts.next();
    let (version, source_start) = if second.is_some_and(|value| {
        value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit())
    }) {
        (second.map(str::to_string), parts.next())
    } else {
        (None, second)
    };
    let source = source_start.map(|source| source.trim_matches(['(', ')']).to_string());
    CargoDependencyRequest {
        name,
        version,
        source,
    }
}
