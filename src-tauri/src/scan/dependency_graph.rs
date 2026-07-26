use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;
use toml::Value as TomlValue;

use crate::{
    error::AppError,
    models::{
        DependencyGraphCompleteness, DependencyGraphEdge, DependencyGraphNode,
        DependencyGraphNodeKind, DependencyGraphSummary, ProjectDependencyGraph, ProjectMetadata,
    },
};

const MAX_LOCK_FILE_BYTES: u64 = 25 * 1024 * 1024;
const MAX_GRAPH_NODES: usize = 50_000;
const MAX_GRAPH_EDGES: usize = 200_000;

#[derive(Debug)]
struct ParsedGraph {
    nodes: BTreeMap<String, DependencyGraphNode>,
    edges: Vec<DependencyGraphEdge>,
    raw_node_count: usize,
    completeness: DependencyGraphCompleteness,
    warnings: Vec<String>,
    source: String,
    digest: String,
}

impl ParsedGraph {
    fn empty(source: &str, completeness: DependencyGraphCompleteness, warning: String) -> Self {
        Self {
            nodes: BTreeMap::new(),
            edges: Vec::new(),
            raw_node_count: 0,
            completeness,
            warnings: vec![warning],
            source: source.into(),
            digest: String::new(),
        }
    }
}

pub fn build_project_dependency_graph(
    project: &ProjectMetadata,
    cancelled: &AtomicBool,
) -> Result<ProjectDependencyGraph, AppError> {
    let directory = Path::new(&project.path);
    let root_id = format!("project:{}", stable_digest(project.path.as_bytes()));
    let mut nodes = BTreeMap::new();
    nodes.insert(
        root_id.clone(),
        DependencyGraphNode {
            id: root_id.clone(),
            ecosystem: "Project".into(),
            name: project.name.clone(),
            version: String::new(),
            kind: DependencyGraphNodeKind::Project,
            direct: false,
            scopes: Vec::new(),
            package_url: None,
        },
    );

    let mut parsed = Vec::new();
    for candidate in select_lock_candidates(project, directory) {
        match candidate {
            LockCandidate::Npm(path) => {
                parsed.push(parse_npm_lock(&path, directory, &root_id, cancelled)?)
            }
            LockCandidate::Pnpm(path) => {
                parsed.push(parse_pnpm_lock(&path, directory, &root_id, cancelled)?)
            }
            LockCandidate::Cargo(path) => {
                parsed.push(parse_cargo_lock(&path, project, &root_id, cancelled)?)
            }
            LockCandidate::AmbiguousJavaScript => parsed.push(ParsedGraph::empty(
                "JavaScript 锁文件",
                DependencyGraphCompleteness::Partial,
                "检测到多个 JavaScript 锁文件，且没有可确定的 packageManager 声明。".into(),
            )),
        }
    }

    if parsed.is_empty() {
        let summary = DependencyGraphSummary {
            node_count: 1,
            edge_count: 0,
            direct_count: 0,
            transitive_count: 0,
            duplicate_version_count: 0,
            unreachable_count: 0,
            cycle_count: 0,
            completeness: DependencyGraphCompleteness::Unsupported,
            sources: Vec::new(),
            source_digest: String::new(),
        };
        return Ok(ProjectDependencyGraph {
            project_name: project.name.clone(),
            project_path: project.path.clone(),
            completeness: DependencyGraphCompleteness::Unsupported,
            sources: Vec::new(),
            source_digest: String::new(),
            nodes: nodes.into_values().collect(),
            edges: Vec::new(),
            warnings: vec!["当前项目没有受支持的完整依赖图锁文件。".into()],
            summary,
        });
    }

    let mut edges = Vec::new();
    let mut warnings = Vec::new();
    let mut sources = Vec::new();
    let mut digests = Vec::new();
    let mut raw_node_count = 0usize;
    let mut completeness = DependencyGraphCompleteness::Complete;
    for graph in parsed {
        if cancelled.load(Ordering::SeqCst) {
            return Err(AppError::ScanCancelled);
        }
        raw_node_count = raw_node_count.saturating_add(graph.raw_node_count);
        completeness = merge_completeness(completeness, graph.completeness);
        sources.push(graph.source.clone());
        if !graph.digest.is_empty() {
            digests.push(format!("{}:{}", graph.source, graph.digest));
        }
        warnings.extend(graph.warnings);
        for (id, node) in graph.nodes {
            if nodes.len() >= MAX_GRAPH_NODES && !nodes.contains_key(&id) {
                completeness =
                    merge_completeness(completeness, DependencyGraphCompleteness::Partial);
                push_unique(
                    &mut warnings,
                    format!("依赖节点超过 {MAX_GRAPH_NODES} 个，结果已截断。"),
                );
                break;
            }
            nodes.entry(id).or_insert(node);
        }
        for edge in graph.edges {
            if edges.len() >= MAX_GRAPH_EDGES {
                completeness =
                    merge_completeness(completeness, DependencyGraphCompleteness::Partial);
                push_unique(
                    &mut warnings,
                    format!("依赖关系超过 {MAX_GRAPH_EDGES} 条，结果已截断。"),
                );
                break;
            }
            if nodes.contains_key(&edge.from) && nodes.contains_key(&edge.to) {
                edges.push(edge);
            }
        }
    }
    sources.sort();
    sources.dedup();
    edges.sort_by(|left, right| {
        left.from
            .cmp(&right.from)
            .then_with(|| left.to.cmp(&right.to))
            .then_with(|| left.dependency_type.cmp(&right.dependency_type))
    });
    edges.dedup_by(|left, right| {
        left.from == right.from
            && left.to == right.to
            && left.dependency_type == right.dependency_type
    });
    let source_digest = if digests.is_empty() {
        String::new()
    } else {
        stable_digest(digests.join("|").as_bytes())
    };
    let mut graph_nodes = nodes.into_values().collect::<Vec<_>>();
    graph_nodes.sort_by(|left, right| {
        node_kind_order(left.kind)
            .cmp(&node_kind_order(right.kind))
            .then_with(|| left.ecosystem.cmp(&right.ecosystem))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.version.cmp(&right.version))
    });
    let duplicate_version_count = duplicate_version_count(&graph_nodes);
    let cycle_count = dependency_cycle_count(&graph_nodes, &edges);
    let reachable_package_count = graph_nodes
        .iter()
        .filter(|node| node.kind != DependencyGraphNodeKind::Project)
        .count();
    let unreachable_count = raw_node_count.saturating_sub(reachable_package_count);
    let direct_count = graph_nodes.iter().filter(|node| node.direct).count();
    let summary = DependencyGraphSummary {
        node_count: graph_nodes.len(),
        edge_count: edges.len(),
        direct_count,
        transitive_count: reachable_package_count.saturating_sub(direct_count),
        duplicate_version_count,
        unreachable_count,
        cycle_count,
        completeness,
        sources: sources.clone(),
        source_digest: source_digest.clone(),
    };
    Ok(ProjectDependencyGraph {
        project_name: project.name.clone(),
        project_path: project.path.clone(),
        completeness,
        sources,
        source_digest,
        nodes: graph_nodes,
        edges,
        warnings,
        summary,
    })
}

enum LockCandidate {
    Npm(PathBuf),
    Pnpm(PathBuf),
    Cargo(PathBuf),
    AmbiguousJavaScript,
}

fn select_lock_candidates(project: &ProjectMetadata, directory: &Path) -> Vec<LockCandidate> {
    let mut candidates = Vec::new();
    let npm_lock = find_lock_file(directory, "package-lock.json");
    let pnpm_lock = find_lock_file(directory, "pnpm-lock.yaml");
    let manager = project.package_manager.as_deref().unwrap_or_default();
    if manager.starts_with("pnpm") {
        if let Some(path) = pnpm_lock {
            candidates.push(LockCandidate::Pnpm(path));
        }
    } else if manager.starts_with("npm") {
        if let Some(path) = npm_lock {
            candidates.push(LockCandidate::Npm(path));
        }
    } else {
        match (npm_lock, pnpm_lock) {
            (Some(path), None) => candidates.push(LockCandidate::Npm(path)),
            (None, Some(path)) => candidates.push(LockCandidate::Pnpm(path)),
            (Some(_), Some(_)) => candidates.push(LockCandidate::AmbiguousJavaScript),
            (None, None) => {}
        }
    }
    if project.ecosystems.iter().any(|item| item == "Rust") {
        if let Some(path) = find_lock_file(directory, "Cargo.lock") {
            candidates.push(LockCandidate::Cargo(path));
        }
    }
    candidates
}

/// 用与完整图构建相同的来源选择与摘要算法计算 source digest，但只读取锁文件、
/// 不解析。返回 None 表示无法快速判定（读取失败），调用方应视为缓存未命中。
/// 无效锁文件会得到与解析路径不同的 digest —— 只会造成多余重建，不会错误复用。
pub(super) fn quick_source_digest(project: &ProjectMetadata) -> Option<String> {
    let directory = PathBuf::from(&project.path);
    let mut digests = Vec::new();
    for candidate in select_lock_candidates(project, &directory) {
        let (source_name, path) = match candidate {
            LockCandidate::Npm(path) => ("package-lock.json", path),
            LockCandidate::Pnpm(path) => ("pnpm-lock.yaml", path),
            LockCandidate::Cargo(path) => ("Cargo.lock", path),
            LockCandidate::AmbiguousJavaScript => continue,
        };
        match read_lock_file(&path, source_name) {
            Ok(LockSource::Content(source)) => {
                digests.push(format!(
                    "{source_name}:{}",
                    stable_digest(source.as_bytes())
                ));
            }
            Ok(LockSource::TooLarge(_)) => {}
            Err(_) => return None,
        }
    }
    Some(if digests.is_empty() {
        String::new()
    } else {
        stable_digest(digests.join("|").as_bytes())
    })
}

/// 以上一次扫描的摘要为缓存：锁文件 digest 未变的项目直接复用摘要，
/// 只为新增或锁文件变化的项目构建完整依赖图。
pub fn enrich_dependency_graph_summaries(
    projects: &mut [ProjectMetadata],
    cancelled: &AtomicBool,
    previous_projects: &[ProjectMetadata],
) -> Result<(), AppError> {
    let previous_by_path: std::collections::HashMap<&str, &ProjectMetadata> = previous_projects
        .iter()
        .map(|project| (project.path.as_str(), project))
        .collect();
    for project in projects {
        if cancelled.load(Ordering::SeqCst) {
            return Err(AppError::ScanCancelled);
        }
        if let Some(previous) = previous_by_path.get(project.path.as_str()) {
            if let Some(previous_summary) = previous.dependency_graph_summary.as_ref() {
                if !previous_summary.source_digest.is_empty()
                    && quick_source_digest(project).as_deref()
                        == Some(previous_summary.source_digest.as_str())
                {
                    project.dependency_graph_summary = Some(previous_summary.clone());
                    project.supply_chain_risk_summary = previous.supply_chain_risk_summary.clone();
                    continue;
                }
            }
        }
        let graph = build_project_dependency_graph(project, cancelled)?;
        let risk_summary = super::supply_chain::build_supply_chain_report(project, &graph).summary;
        project.dependency_graph_summary = (graph.completeness
            != DependencyGraphCompleteness::Unsupported)
            .then_some(graph.summary);
        project.supply_chain_risk_summary = (risk_summary.total_count > 0).then_some(risk_summary);
    }
    Ok(())
}

fn parse_npm_lock(
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

fn parse_pnpm_lock(
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

fn parse_cargo_lock(
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

enum LockSource {
    Content(String),
    TooLarge(u64),
}

fn read_lock_file(path: &Path, source_name: &str) -> Result<LockSource, AppError> {
    read_lock_file_with_limit(path, source_name, MAX_LOCK_FILE_BYTES)
}

fn read_lock_file_with_limit(
    path: &Path,
    source_name: &str,
    max_bytes: u64,
) -> Result<LockSource, AppError> {
    let metadata = fs::metadata(path)
        .map_err(|error| AppError::Command(format!("无法读取 {source_name} 元数据：{error}")))?;
    if metadata.len() > max_bytes {
        return Ok(LockSource::TooLarge(metadata.len()));
    }
    fs::read_to_string(path)
        .map(LockSource::Content)
        .map_err(|error| AppError::Command(format!("无法读取 {source_name}：{error}")))
}

fn find_lock_file(directory: &Path, name: &str) -> Option<PathBuf> {
    directory
        .ancestors()
        .map(|ancestor| ancestor.join(name))
        .find(|path| path.is_file())
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

fn package_node(
    id: &str,
    ecosystem: &str,
    name: &str,
    version: &str,
    direct: bool,
) -> DependencyGraphNode {
    DependencyGraphNode {
        id: id.into(),
        ecosystem: ecosystem.into(),
        name: name.into(),
        version: version.into(),
        kind: DependencyGraphNodeKind::Package,
        direct,
        scopes: Vec::new(),
        package_url: package_url(ecosystem, name, version),
    }
}

fn package_url(ecosystem: &str, name: &str, version: &str) -> Option<String> {
    if version.is_empty() || version == "local" || version == "未知" {
        return None;
    }
    match ecosystem {
        "JavaScript" => Some(format!("pkg:npm/{}@{version}", name.replace('@', "%40"))),
        "Rust" => Some(format!("pkg:cargo/{name}@{version}")),
        _ => None,
    }
}

fn mark_direct(nodes: &mut BTreeMap<String, DependencyGraphNode>, id: &str, scope: &str) {
    if let Some(node) = nodes.get_mut(id) {
        node.direct = true;
        if !node.scopes.iter().any(|item| item == scope) {
            node.scopes.push(scope.into());
            node.scopes.sort();
        }
    }
}

fn push_edge(
    edges: &mut Vec<DependencyGraphEdge>,
    from: &str,
    to: &str,
    dependency_type: &str,
    completeness: &mut DependencyGraphCompleteness,
    warnings: &mut Vec<String>,
) {
    if edges.len() >= MAX_GRAPH_EDGES {
        *completeness = merge_completeness(*completeness, DependencyGraphCompleteness::Partial);
        push_unique(
            warnings,
            format!("依赖关系超过 {MAX_GRAPH_EDGES} 条，结果已截断。"),
        );
        return;
    }
    edges.push(DependencyGraphEdge {
        from: from.into(),
        to: to.into(),
        dependency_type: dependency_type.into(),
    });
}

fn prune_unreachable(
    root_id: &str,
    nodes: &mut BTreeMap<String, DependencyGraphNode>,
    edges: &mut Vec<DependencyGraphEdge>,
) {
    let mut adjacency = HashMap::<&str, Vec<&str>>::new();
    for edge in edges.iter() {
        adjacency
            .entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::from([root_id]);
    while let Some(current) = queue.pop_front() {
        if !reachable.insert(current.to_string()) {
            continue;
        }
        if let Some(targets) = adjacency.get(current) {
            queue.extend(targets.iter().copied());
        }
    }
    nodes.retain(|id, _| reachable.contains(id));
    edges.retain(|edge| reachable.contains(&edge.from) && reachable.contains(&edge.to));
}

fn duplicate_version_count(nodes: &[DependencyGraphNode]) -> usize {
    let mut versions = BTreeMap::<(&str, &str), BTreeSet<&str>>::new();
    for node in nodes
        .iter()
        .filter(|node| node.kind == DependencyGraphNodeKind::Package)
    {
        versions
            .entry((&node.ecosystem, &node.name))
            .or_default()
            .insert(&node.version);
    }
    versions
        .values()
        .filter(|versions| versions.len() > 1)
        .count()
}

fn dependency_cycle_count(nodes: &[DependencyGraphNode], edges: &[DependencyGraphEdge]) -> usize {
    let ids = nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<HashSet<_>>();
    let mut adjacency = HashMap::<&str, Vec<&str>>::new();
    for edge in edges {
        if ids.contains(edge.from.as_str()) && ids.contains(edge.to.as_str()) {
            adjacency
                .entry(edge.from.as_str())
                .or_default()
                .push(edge.to.as_str());
        }
    }
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    let mut back_edges = 0usize;
    for node in &ids {
        count_cycle_edges(
            node,
            &adjacency,
            &mut visiting,
            &mut visited,
            &mut back_edges,
        );
    }
    back_edges
}

fn count_cycle_edges<'a>(
    node: &'a str,
    adjacency: &HashMap<&'a str, Vec<&'a str>>,
    visiting: &mut HashSet<&'a str>,
    visited: &mut HashSet<&'a str>,
    back_edges: &mut usize,
) {
    if visited.contains(node) {
        return;
    }
    if !visiting.insert(node) {
        *back_edges += 1;
        return;
    }
    if let Some(targets) = adjacency.get(node) {
        for target in targets {
            if visiting.contains(target) {
                *back_edges += 1;
            } else {
                count_cycle_edges(target, adjacency, visiting, visited, back_edges);
            }
        }
    }
    visiting.remove(node);
    visited.insert(node);
}

fn merge_completeness(
    left: DependencyGraphCompleteness,
    right: DependencyGraphCompleteness,
) -> DependencyGraphCompleteness {
    use DependencyGraphCompleteness::{Complete, Invalid, Partial, Unsupported};
    match (left, right) {
        (Invalid, _) | (_, Invalid) => Invalid,
        (Partial, _) | (_, Partial) => Partial,
        (Unsupported, value) | (value, Unsupported) => value,
        (Complete, Complete) => Complete,
    }
}

fn node_kind_order(kind: DependencyGraphNodeKind) -> u8 {
    match kind {
        DependencyGraphNodeKind::Project => 0,
        DependencyGraphNodeKind::Package => 1,
        DependencyGraphNodeKind::Local => 2,
    }
}

fn stable_digest(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

#[cfg(test)]
fn build_cyclonedx_sbom(graph: &ProjectDependencyGraph) -> Result<String, AppError> {
    build_cyclonedx_sbom_with_risk_summary(graph, None)
}

pub fn build_cyclonedx_sbom_with_risk_summary(
    graph: &ProjectDependencyGraph,
    risk_summary: Option<&crate::models::SupplyChainRiskSummary>,
) -> Result<String, AppError> {
    let components = graph
        .nodes
        .iter()
        .filter(|node| node.kind != DependencyGraphNodeKind::Project)
        .map(|node| {
            let mut component = serde_json::json!({
                "type": if node.kind == DependencyGraphNodeKind::Local { "application" } else { "library" },
                "bom-ref": node.id,
                "name": node.name,
                "version": node.version,
                "properties": [
                    { "name": "easy-package:ecosystem", "value": node.ecosystem },
                    { "name": "easy-package:direct", "value": node.direct.to_string() }
                ]
            });
            if let Some(package_url) = &node.package_url {
                component["purl"] = JsonValue::String(package_url.clone());
            }
            component
        })
        .collect::<Vec<_>>();
    let mut dependencies = BTreeMap::<String, BTreeSet<String>>::new();
    for node in &graph.nodes {
        dependencies.entry(node.id.clone()).or_default();
    }
    for edge in &graph.edges {
        dependencies
            .entry(edge.from.clone())
            .or_default()
            .insert(edge.to.clone());
    }
    let dependencies = dependencies
        .into_iter()
        .map(|(reference, targets)| serde_json::json!({ "ref": reference, "dependsOn": targets }))
        .collect::<Vec<_>>();
    let root = graph
        .nodes
        .iter()
        .find(|node| node.kind == DependencyGraphNodeKind::Project)
        .ok_or_else(|| AppError::Serialization("依赖图缺少项目根节点".into()))?;
    let mut properties = vec![
        serde_json::json!({ "name": "easy-package:source", "value": graph.sources.join(",") }),
        serde_json::json!({ "name": "easy-package:completeness", "value": format!("{:?}", graph.completeness).to_lowercase() }),
        serde_json::json!({ "name": "easy-package:scope", "value": "lockfile-only" }),
    ];
    if let Some(summary) = risk_summary {
        properties.extend([
            serde_json::json!({ "name": "easy-package:risk-total", "value": summary.total_count.to_string() }),
            serde_json::json!({ "name": "easy-package:risk-warnings", "value": summary.warning_count.to_string() }),
            serde_json::json!({ "name": "easy-package:risk-rule-ids", "value": summary.rule_ids.join(",") }),
            serde_json::json!({ "name": "easy-package:risk-scope", "value": "offline-structural-only" }),
        ]);
    }
    let report = serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "serialNumber": format!("urn:uuid:{}", uuid::Uuid::new_v4()),
        "version": 1,
        "metadata": {
            "component": {
                "type": "application",
                "bom-ref": root.id,
                "name": graph.project_name
            },
            "tools": [{ "vendor": "Easy Package", "name": "Easy Package" }],
            "properties": properties
        },
        "components": components,
        "dependencies": dependencies
    });
    Ok(serde_json::to_string_pretty(&report)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ProjectDependency, RuntimeRequirement};
    use tempfile::tempdir;

    fn project(path: &Path, name: &str, ecosystem: &str, manager: Option<&str>) -> ProjectMetadata {
        ProjectMetadata {
            name: name.into(),
            path: path.to_string_lossy().into_owned(),
            ecosystems: vec![ecosystem.into()],
            lock_files: Vec::new(),
            runtime_requirements: Vec::<RuntimeRequirement>::new(),
            package_manager: manager.map(str::to_string),
            dependencies: Vec::<ProjectDependency>::new(),
            workspace: None,
            dependency_graph_summary: None,
            supply_chain_risk_summary: None,
            warnings: Vec::new(),
        }
    }

    #[test]
    #[ignore]
    fn bench_enrich_cold_vs_warm() {
        let directory = tempdir().unwrap();
        let mut projects = Vec::new();
        for index in 0..30 {
            let project_dir = directory.path().join(format!("proj-{index}"));
            fs::create_dir_all(&project_dir).unwrap();
            let mut packages = String::from(
                r#"{"lockfileVersion":3,"packages":{"":{"name":"app","dependencies":{"#,
            );
            for dep in 0..50 {
                if dep > 0 {
                    packages.push(',');
                }
                packages.push_str(&format!(r#""dep-{dep}":"1""#));
            }
            packages.push_str("}}");
            for dep in 0..800 {
                packages.push_str(&format!(
                    r#","node_modules/dep-{dep}":{{"name":"dep-{dep}","version":"1.0.{index}"}}"#
                ));
            }
            packages.push_str("}}");
            fs::write(project_dir.join("package-lock.json"), packages).unwrap();
            projects.push(project(
                &project_dir,
                &format!("proj-{index}"),
                "JavaScript",
                Some("npm@11"),
            ));
        }
        let start = std::time::Instant::now();
        enrich_dependency_graph_summaries(&mut projects, &AtomicBool::new(false), &[]).unwrap();
        let cold = start.elapsed();
        let previous = projects.clone();
        let mut rescanned = previous
            .iter()
            .map(|item| ProjectMetadata {
                dependency_graph_summary: None,
                supply_chain_risk_summary: None,
                ..item.clone()
            })
            .collect::<Vec<_>>();
        let start = std::time::Instant::now();
        enrich_dependency_graph_summaries(&mut rescanned, &AtomicBool::new(false), &previous)
            .unwrap();
        let warm = start.elapsed();
        println!("cold(30 projects x ~800 nodes): {cold:?}, warm(digest hit): {warm:?}");
        assert!(rescanned
            .iter()
            .all(|item| item.dependency_graph_summary.is_some()));
    }

    #[test]
    fn quick_digest_matches_full_graph_digest() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"":{"name":"app","dependencies":{"a":"1"}},"node_modules/a":{"name":"a","version":"1.0.0"}}}"#,
        )
        .unwrap();
        let metadata = project(directory.path(), "app", "JavaScript", Some("npm@11"));
        let graph = build_project_dependency_graph(&metadata, &AtomicBool::new(false)).unwrap();
        assert!(!graph.source_digest.is_empty());
        assert_eq!(
            quick_source_digest(&metadata).as_deref(),
            Some(graph.source_digest.as_str())
        );

        fs::write(directory.path().join("package-lock.json"), "{}").unwrap();
        assert_ne!(
            quick_source_digest(&metadata).as_deref(),
            Some(graph.source_digest.as_str())
        );
    }

    #[test]
    fn enrich_reuses_previous_summary_when_digest_is_unchanged() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"":{"name":"app","dependencies":{"a":"1"}},"node_modules/a":{"name":"a","version":"1.0.0"}}}"#,
        )
        .unwrap();
        let mut projects = vec![project(
            directory.path(),
            "app",
            "JavaScript",
            Some("npm@11"),
        )];
        enrich_dependency_graph_summaries(&mut projects, &AtomicBool::new(false), &[]).unwrap();
        let built = projects[0].dependency_graph_summary.clone().unwrap();

        // 上一次快照携带同 digest 但哨兵计数的摘要：digest 命中时必须原样复用，证明未重建。
        let mut previous = projects.clone();
        let sentinel = previous[0].dependency_graph_summary.as_mut().unwrap();
        sentinel.node_count = 999;
        let mut rescanned = vec![project(
            directory.path(),
            "app",
            "JavaScript",
            Some("npm@11"),
        )];
        enrich_dependency_graph_summaries(&mut rescanned, &AtomicBool::new(false), &previous)
            .unwrap();
        assert_eq!(
            rescanned[0]
                .dependency_graph_summary
                .as_ref()
                .unwrap()
                .node_count,
            999
        );

        // 锁文件变化后 digest 失配，重建得到真实值。
        fs::write(
            directory.path().join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"":{"name":"app","dependencies":{"a":"1","b":"1"}},"node_modules/a":{"name":"a","version":"1.0.0"},"node_modules/b":{"name":"b","version":"1.0.0"}}}"#,
        )
        .unwrap();
        let mut changed = vec![project(
            directory.path(),
            "app",
            "JavaScript",
            Some("npm@11"),
        )];
        enrich_dependency_graph_summaries(&mut changed, &AtomicBool::new(false), &previous)
            .unwrap();
        let rebuilt = changed[0].dependency_graph_summary.as_ref().unwrap();
        assert_ne!(rebuilt.node_count, 999);
        assert_eq!(rebuilt.node_count, built.node_count + 1);
    }

    #[test]
    fn parses_npm_hoisting_nested_scoped_and_optional_dependencies() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("package-lock.json"),
            r#"{
              "lockfileVersion":3,
              "packages":{
                "":{"name":"app","dependencies":{"a":"1","@scope/c":"1"},"optionalDependencies":{"opt":"1"}},
                "node_modules/a":{"name":"a","version":"1.0.0","dependencies":{"b":"1"}},
                "node_modules/a/node_modules/b":{"name":"b","version":"1.0.0"},
                "node_modules/@scope/c":{"name":"@scope/c","version":"2.0.0","dependencies":{"b":"2"}},
                "node_modules/b":{"name":"b","version":"2.0.0"},
                "node_modules/opt":{"name":"opt","version":"3.0.0"},
                "node_modules/unreachable":{"name":"unreachable","version":"9.0.0"}
              }
            }"#,
        )
        .unwrap();
        let graph = build_project_dependency_graph(
            &project(directory.path(), "app", "JavaScript", Some("npm@11")),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(graph.completeness, DependencyGraphCompleteness::Complete);
        assert!(graph.nodes.iter().any(|node| node.name == "@scope/c"));
        assert_eq!(graph.summary.duplicate_version_count, 1);
        assert_eq!(graph.summary.unreachable_count, 1);
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.dependency_type == "optional"));
    }

    #[test]
    fn parses_pnpm_v9_importer_peer_variant_and_workspace_boundary() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '9.0'\nimporters:\n  .:\n    dependencies:\n      react:\n        version: 19.1.1\n      local-ui:\n        version: link:packages/ui\npackages:\n  react@19.1.1: {}\n  scheduler@0.26.0: {}\nsnapshots:\n  react@19.1.1:\n    dependencies:\n      scheduler: 0.26.0\n  scheduler@0.26.0: {}\n",
        )
        .unwrap();
        let graph = build_project_dependency_graph(
            &project(directory.path(), "app", "JavaScript", Some("pnpm@11")),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(graph
            .nodes
            .iter()
            .any(|node| node.name == "react" && node.direct));
        assert!(graph
            .nodes
            .iter()
            .any(|node| node.kind == DependencyGraphNodeKind::Local));
        assert!(graph.nodes.iter().any(|node| node.name == "scheduler"));
    }

    #[test]
    fn parses_pnpm_alias_and_peer_variant() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '9.0'\nimporters:\n  .:\n    dependencies:\n      pretty-react:\n        version: npm:react@19.1.1\npackages:\n  react@19.1.1(typescript@5.9.3): {}\nsnapshots:\n  react@19.1.1(typescript@5.9.3): {}\n",
        )
        .unwrap();
        let graph = build_project_dependency_graph(
            &project(directory.path(), "app", "JavaScript", Some("pnpm@11")),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(graph
            .nodes
            .iter()
            .any(|node| node.name == "react" && node.version == "19.1.1" && node.direct));
    }

    #[test]
    fn ambiguous_javascript_locks_do_not_fabricate_source_digest() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join("package-lock.json"), "{}").unwrap();
        fs::write(directory.path().join("pnpm-lock.yaml"), "{}").unwrap();
        let graph = build_project_dependency_graph(
            &project(directory.path(), "app", "JavaScript", None),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(graph.completeness, DependencyGraphCompleteness::Partial);
        assert!(graph.source_digest.is_empty());
    }

    #[test]
    fn parses_cargo_duplicate_versions_and_cycle() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("Cargo.lock"),
            r#"version = 4
[[package]]
name = "app"
version = "0.1.0"
dependencies = ["a 1.0.0", "a 2.0.0"]
[[package]]
name = "a"
version = "1.0.0"
dependencies = ["b 1.0.0"]
[[package]]
name = "a"
version = "2.0.0"
[[package]]
name = "b"
version = "1.0.0"
dependencies = ["a 1.0.0"]
"#,
        )
        .unwrap();
        let graph = build_project_dependency_graph(
            &project(directory.path(), "app", "Rust", None),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(graph.summary.duplicate_version_count, 1);
        assert!(graph.summary.cycle_count >= 1);
    }

    #[test]
    fn rejects_invalid_lock_and_honors_cancellation() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join("package-lock.json"), "not-json").unwrap();
        let project = project(directory.path(), "app", "JavaScript", Some("npm@11"));
        let graph = build_project_dependency_graph(&project, &AtomicBool::new(false)).unwrap();
        assert_eq!(graph.completeness, DependencyGraphCompleteness::Invalid);
        assert!(matches!(
            build_project_dependency_graph(&project, &AtomicBool::new(true)),
            Err(AppError::ScanCancelled)
        ));
    }

    #[test]
    fn enforces_lock_file_read_budget_without_large_fixture() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("package-lock.json");
        fs::write(&path, "123456789").unwrap();
        assert!(matches!(
            read_lock_file_with_limit(&path, "package-lock.json", 4).unwrap(),
            LockSource::TooLarge(9)
        ));
    }

    #[test]
    fn exports_cyclonedx_without_local_paths() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"":{"name":"app","dependencies":{"react":"19"}},"node_modules/react":{"name":"react","version":"19.1.1"}}}"#,
        )
        .unwrap();
        let graph = build_project_dependency_graph(
            &project(directory.path(), "app", "JavaScript", Some("npm@11")),
            &AtomicBool::new(false),
        )
        .unwrap();
        let report = build_cyclonedx_sbom(&graph).unwrap();
        let value: JsonValue = serde_json::from_str(&report).unwrap();
        assert_eq!(value["bomFormat"], "CycloneDX");
        assert_eq!(value["specVersion"], "1.6");
        assert!(report.contains("pkg:npm/react@19.1.1"));
        assert!(!report.contains(directory.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn exports_only_offline_structural_risk_summary() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"":{"name":"app","dependencies":{"react":"19"}},"node_modules/react":{"name":"react","version":"19.1.1"}}}"#,
        )
        .unwrap();
        let graph = build_project_dependency_graph(
            &project(directory.path(), "app", "JavaScript", Some("npm@11")),
            &AtomicBool::new(false),
        )
        .unwrap();
        let summary = crate::models::SupplyChainRiskSummary {
            total_count: 1,
            warning_count: 1,
            info_count: 0,
            rule_ids: vec!["MULTIPLE_RESOLVED_VERSIONS".into()],
        };
        let report = build_cyclonedx_sbom_with_risk_summary(&graph, Some(&summary)).unwrap();
        assert!(report.contains("easy-package:risk-scope"));
        assert!(report.contains("offline-structural-only"));
        assert!(!report.to_lowercase().contains("vulnerability"));
        assert!(!report.to_lowercase().contains("license"));
    }
}
