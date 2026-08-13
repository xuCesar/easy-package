//! 完整依赖图构建：来源选择、按需缓存、图组装与扫描摘要复用。
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
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

mod parsers;
mod sbom;

use parsers::{parse_cargo_lock, parse_npm_lock, parse_pnpm_lock};
#[cfg(test)]
use sbom::build_cyclonedx_sbom;
pub use sbom::build_cyclonedx_sbom_with_risk_summary;

const MAX_LOCK_FILE_BYTES: u64 = 25 * 1024 * 1024;
const MAX_GRAPH_NODES: usize = 50_000;
const MAX_GRAPH_EDGES: usize = 200_000;
const GRAPH_CACHE_CAPACITY: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
struct GraphCacheKey {
    project_path: String,
    project_name: String,
    project_input_digest: String,
    source_digest: String,
}

#[derive(Debug, Clone)]
struct GraphCacheEntry {
    key: GraphCacheKey,
    graph: ProjectDependencyGraph,
}

#[derive(Debug)]
struct GraphCacheState {
    entries: VecDeque<GraphCacheEntry>,
    capacity: usize,
}

/// 完整依赖图只保存在当前进程内，避免把大图写入 SQLite。
/// Mutex 在构建期间保持占用，确保图、锁文件问题与 SBOM 并发请求只解析一次。
#[derive(Debug, Clone)]
pub struct DependencyGraphCache {
    state: Arc<Mutex<GraphCacheState>>,
}

impl Default for DependencyGraphCache {
    fn default() -> Self {
        Self::with_capacity(GRAPH_CACHE_CAPACITY)
    }
}

impl DependencyGraphCache {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(GraphCacheState {
                entries: VecDeque::new(),
                capacity: capacity.max(1),
            })),
        }
    }

    pub fn get_or_build(
        &self,
        project: &ProjectMetadata,
        cancelled: &AtomicBool,
    ) -> Result<ProjectDependencyGraph, AppError> {
        self.get_or_build_with(project, cancelled, build_project_dependency_graph)
    }

    fn get_or_build_with<F>(
        &self,
        project: &ProjectMetadata,
        cancelled: &AtomicBool,
        builder: F,
    ) -> Result<ProjectDependencyGraph, AppError>
    where
        F: FnOnce(&ProjectMetadata, &AtomicBool) -> Result<ProjectDependencyGraph, AppError>,
    {
        if cancelled.load(Ordering::SeqCst) {
            return Err(AppError::ScanCancelled);
        }
        let source_digest = quick_source_digest(project).unwrap_or_default();
        if source_digest.is_empty() {
            return builder(project, cancelled);
        }
        let key = GraphCacheKey {
            project_path: project.path.clone(),
            project_name: project.name.clone(),
            project_input_digest: project_input_digest(project),
            source_digest: source_digest.clone(),
        };
        let mut state = self
            .state
            .lock()
            .map_err(|error| AppError::Command(error.to_string()))?;
        if let Some(index) = state.entries.iter().position(|entry| entry.key == key) {
            let entry = state.entries.remove(index).expect("缓存索引必须存在");
            let graph = entry.graph.clone();
            state.entries.push_front(entry);
            return Ok(graph);
        }

        let graph = builder(project, cancelled)?;
        // 解析前后锁文件发生变化时仍返回本次结果，但不缓存不确定版本。
        if graph.source_digest == source_digest {
            state.entries.retain(|entry| {
                entry.key.project_path != key.project_path
                    || entry.key.project_name != key.project_name
            });
            state.entries.push_front(GraphCacheEntry {
                key,
                graph: graph.clone(),
            });
            while state.entries.len() > state.capacity {
                state.entries.pop_back();
            }
        }
        Ok(graph)
    }
}

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
pub(crate) fn quick_source_digest(project: &ProjectMetadata) -> Option<String> {
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

/// 只复用上一次扫描中 digest 仍匹配的摘要；冷扫描或锁文件变化时保持待分析，
/// 完整图统一留给项目详情按需构建。
pub fn reuse_dependency_graph_summaries(
    projects: &mut [ProjectMetadata],
    cancelled: &AtomicBool,
    previous_projects: &[ProjectMetadata],
) -> Result<(), AppError> {
    if cancelled.load(Ordering::SeqCst) {
        return Err(AppError::ScanCancelled);
    }
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
                    && same_project_graph_inputs(previous, project)
                    && quick_source_digest(project).as_deref()
                        == Some(previous_summary.source_digest.as_str())
                {
                    project.dependency_graph_summary = Some(previous_summary.clone());
                    project.supply_chain_risk_summary = previous.supply_chain_risk_summary.clone();
                    continue;
                }
            }
        }
    }
    Ok(())
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

fn project_input_digest(project: &ProjectMetadata) -> String {
    let serialized = serde_json::to_vec(&(
        project.name.as_str(),
        &project.ecosystems,
        project.package_manager.as_deref(),
        &project.dependencies,
    ))
    .unwrap_or_default();
    stable_digest(&serialized)
}

fn same_project_graph_inputs(previous: &ProjectMetadata, current: &ProjectMetadata) -> bool {
    previous.name == current.name
        && previous.ecosystems == current.ecosystems
        && previous.package_manager == current.package_manager
        && previous.dependencies == current.dependencies
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ProjectDependency, RuntimeRequirement};
    use std::sync::atomic::AtomicUsize;
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
    fn bench_graph_cache_cold_vs_warm() {
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
        let cache = DependencyGraphCache::with_capacity(projects.len());
        let start = std::time::Instant::now();
        for project in &projects {
            cache
                .get_or_build(project, &AtomicBool::new(false))
                .unwrap();
        }
        let cold = start.elapsed();
        let start = std::time::Instant::now();
        for project in &projects {
            cache
                .get_or_build(project, &AtomicBool::new(false))
                .unwrap();
        }
        let warm = start.elapsed();
        println!("cold(30 projects x ~800 nodes): {cold:?}, warm(graph cache): {warm:?}");
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
    fn graph_cache_reuses_same_digest_and_rebuilds_after_change() {
        let directory = tempdir().unwrap();
        let lock_path = directory.path().join("package-lock.json");
        fs::write(
            &lock_path,
            r#"{"lockfileVersion":3,"packages":{"":{"name":"app","dependencies":{"a":"1"}},"node_modules/a":{"name":"a","version":"1.0.0"}}}"#,
        )
        .unwrap();
        let metadata = project(directory.path(), "app", "JavaScript", Some("npm@11"));
        let first_graph =
            build_project_dependency_graph(&metadata, &AtomicBool::new(false)).unwrap();
        let cache = DependencyGraphCache::default();
        let builds = AtomicUsize::new(0);

        for _ in 0..2 {
            cache
                .get_or_build_with(&metadata, &AtomicBool::new(false), |_, _| {
                    builds.fetch_add(1, Ordering::SeqCst);
                    Ok(first_graph.clone())
                })
                .unwrap();
        }
        assert_eq!(builds.load(Ordering::SeqCst), 1);

        fs::write(
            lock_path,
            r#"{"lockfileVersion":3,"packages":{"":{"name":"app","dependencies":{"a":"2"}},"node_modules/a":{"name":"a","version":"2.0.0"}}}"#,
        )
        .unwrap();
        let changed_graph =
            build_project_dependency_graph(&metadata, &AtomicBool::new(false)).unwrap();
        cache
            .get_or_build_with(&metadata, &AtomicBool::new(false), |_, _| {
                builds.fetch_add(1, Ordering::SeqCst);
                Ok(changed_graph.clone())
            })
            .unwrap();
        assert_eq!(builds.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn graph_cache_coalesces_concurrent_builds_and_honors_cancellation() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"":{"name":"app","dependencies":{"a":"1"}},"node_modules/a":{"name":"a","version":"1.0.0"}}}"#,
        )
        .unwrap();
        let metadata = project(directory.path(), "app", "JavaScript", Some("npm@11"));
        let graph = build_project_dependency_graph(&metadata, &AtomicBool::new(false)).unwrap();
        let cache = DependencyGraphCache::default();
        let builds = Arc::new(AtomicUsize::new(0));
        let handles = (0..4)
            .map(|_| {
                let cache = cache.clone();
                let metadata = metadata.clone();
                let graph = graph.clone();
                let builds = builds.clone();
                std::thread::spawn(move || {
                    cache
                        .get_or_build_with(&metadata, &AtomicBool::new(false), |_, _| {
                            builds.fetch_add(1, Ordering::SeqCst);
                            std::thread::sleep(std::time::Duration::from_millis(20));
                            Ok(graph)
                        })
                        .unwrap()
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(builds.load(Ordering::SeqCst), 1);

        let cancelled = AtomicBool::new(true);
        assert!(matches!(
            cache.get_or_build(&metadata, &cancelled),
            Err(AppError::ScanCancelled)
        ));
    }

    #[test]
    fn scan_reuses_previous_summary_only_when_digest_is_unchanged() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"":{"name":"app","dependencies":{"a":"1"}},"node_modules/a":{"name":"a","version":"1.0.0"}}}"#,
        )
        .unwrap();
        let mut previous = vec![project(
            directory.path(),
            "app",
            "JavaScript",
            Some("npm@11"),
        )];
        let graph = build_project_dependency_graph(&previous[0], &AtomicBool::new(false)).unwrap();
        let risk_summary =
            super::super::supply_chain::build_supply_chain_report(&previous[0], &graph).summary;
        previous[0].dependency_graph_summary = Some(graph.summary);
        previous[0].supply_chain_risk_summary = Some(risk_summary);

        // 上一次快照携带同 digest 但哨兵计数的摘要：digest 命中时必须原样复用，证明未重建。
        let sentinel = previous[0].dependency_graph_summary.as_mut().unwrap();
        sentinel.node_count = 999;
        let mut rescanned = vec![project(
            directory.path(),
            "app",
            "JavaScript",
            Some("npm@11"),
        )];
        reuse_dependency_graph_summaries(&mut rescanned, &AtomicBool::new(false), &previous)
            .unwrap();
        assert_eq!(
            rescanned[0]
                .dependency_graph_summary
                .as_ref()
                .unwrap()
                .node_count,
            999
        );

        let mut manifest_changed = vec![project(
            directory.path(),
            "app",
            "JavaScript",
            Some("npm@11"),
        )];
        manifest_changed[0].dependencies.push(ProjectDependency {
            ecosystem: "JavaScript".into(),
            name: "b".into(),
            normalized_name: "b".into(),
            version_requirement: "1".into(),
            scopes: vec!["runtime".into()],
            resolved_version: None,
            resolution_source: None,
            resolution_checked: false,
        });
        reuse_dependency_graph_summaries(&mut manifest_changed, &AtomicBool::new(false), &previous)
            .unwrap();
        assert!(manifest_changed[0].dependency_graph_summary.is_none());

        // 锁文件变化后 digest 失配，扫描保持待分析，不再同步重建完整图。
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
        reuse_dependency_graph_summaries(&mut changed, &AtomicBool::new(false), &previous).unwrap();
        assert!(changed[0].dependency_graph_summary.is_none());
        assert!(changed[0].supply_chain_risk_summary.is_none());
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
