use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::models::{
    DependencyGraphCompleteness, DependencyGraphNode, DependencyGraphNodeKind,
    ProjectDependencyGraph, ProjectMetadata, ProjectSupplyChainReport, SupplyChainRiskFinding,
    SupplyChainRiskSeverity, SupplyChainRiskSummary,
};

pub fn build_supply_chain_report(
    project: &ProjectMetadata,
    graph: &ProjectDependencyGraph,
) -> ProjectSupplyChainReport {
    let mut findings = Vec::new();

    match graph.completeness {
        DependencyGraphCompleteness::Unsupported if !project.dependencies.is_empty() => {
            push_graph_finding(
                &mut findings,
                project,
                "LOCKFILE_MISSING",
                SupplyChainRiskSeverity::Warning,
                "缺少受支持的完整依赖锁文件",
                "项目声明了依赖，但无法从 npm、pnpm 或 Cargo 锁文件重建完整依赖图。",
                graph.sources.clone(),
            )
        }
        DependencyGraphCompleteness::Partial => push_graph_finding(
            &mut findings,
            project,
            "DEPENDENCY_GRAPH_PARTIAL",
            SupplyChainRiskSeverity::Warning,
            "依赖图不完整",
            "锁文件格式、大小或图预算限制导致部分依赖关系未被纳入。",
            graph.warnings.clone(),
        ),
        DependencyGraphCompleteness::Invalid => push_graph_finding(
            &mut findings,
            project,
            "DEPENDENCY_GRAPH_INVALID",
            SupplyChainRiskSeverity::Warning,
            "依赖锁文件无效",
            "受支持的锁文件无法解析，供应链结论不完整。",
            graph.warnings.clone(),
        ),
        _ => {}
    }

    if graph.summary.unreachable_count > 0 {
        push_graph_finding(
            &mut findings,
            project,
            "LOCKFILE_ENTRY_UNREACHABLE",
            SupplyChainRiskSeverity::Info,
            "锁文件包含不可达条目",
            "锁文件中存在无法从项目根节点到达的包条目；它们不会作为当前项目依赖导出。",
            vec![format!("{} 个不可达条目", graph.summary.unreachable_count)],
        );
    }
    if graph.summary.cycle_count > 0 {
        push_graph_finding(
            &mut findings,
            project,
            "DEPENDENCY_CYCLE_DETECTED",
            SupplyChainRiskSeverity::Info,
            "依赖图包含循环回边",
            "锁文件关系中检测到循环回边，请结合依赖路径确认是否为包管理器的正常解析结果。",
            vec![format!("{} 条循环回边", graph.summary.cycle_count)],
        );
    }

    let mut versions =
        BTreeMap::<(String, String), BTreeMap<String, Vec<&DependencyGraphNode>>>::new();
    for node in graph
        .nodes
        .iter()
        .filter(|node| node.kind != DependencyGraphNodeKind::Project)
    {
        versions
            .entry((node.ecosystem.clone(), node.name.to_lowercase()))
            .or_default()
            .entry(node.version.clone())
            .or_default()
            .push(node);

        if node.kind == DependencyGraphNodeKind::Local {
            push_node_finding(
                &mut findings,
                project,
                graph,
                node,
                "NON_REGISTRY_DEPENDENCY",
                SupplyChainRiskSeverity::Info,
                "依赖指向本地或工作区边界",
                "该依赖来自 path、file、link 或 workspace 引用，SBOM 只记录边界，不读取边界外内容。",
                vec![format!("{} {}", node.name, display_version(&node.version))],
            );
        } else if node.package_url.is_none() {
            push_node_finding(
                &mut findings,
                project,
                graph,
                node,
                "PACKAGE_SOURCE_UNKNOWN",
                SupplyChainRiskSeverity::Info,
                "依赖来源无法规范化",
                "锁文件没有提供足够信息来生成 Package URL；这不等同于已确认存在风险。",
                vec![format!("{} {}", node.name, display_version(&node.version))],
            );
        }

        if node.version.trim().is_empty() || matches!(node.version.as_str(), "unknown" | "未解析")
        {
            push_node_finding(
                &mut findings,
                project,
                graph,
                node,
                "DEPENDENCY_VERSION_MISSING",
                SupplyChainRiskSeverity::Warning,
                "依赖缺少已解析版本",
                "无法从锁文件确定该节点的精确版本。",
                vec![node.name.clone()],
            );
        }
    }

    for ((ecosystem, name), by_version) in versions {
        if by_version.len() <= 1 {
            continue;
        }
        let representative = by_version.values().flatten().next().copied();
        if let Some(node) = representative {
            let version_values = by_version.keys().cloned().collect::<Vec<_>>();
            push_node_finding(
                &mut findings,
                project,
                graph,
                node,
                "MULTIPLE_RESOLVED_VERSIONS",
                SupplyChainRiskSeverity::Warning,
                "同一依赖解析为多个版本",
                "依赖图中存在同名包的多个已解析版本，可能增加更新与审计成本。",
                vec![format!(
                    "{ecosystem}:{name} → {}",
                    version_values.join("、")
                )],
            );
        }
    }

    findings.sort_by(|left, right| {
        right
            .severity
            .cmp(&left.severity)
            .then_with(|| left.code.cmp(&right.code))
            .then_with(|| left.id.cmp(&right.id))
    });
    let summary = summarize_findings(&findings);
    ProjectSupplyChainReport {
        project_name: project.name.clone(),
        project_path: project.path.clone(),
        summary,
        findings,
    }
}

pub fn summarize_findings(findings: &[SupplyChainRiskFinding]) -> SupplyChainRiskSummary {
    let warning_count = findings
        .iter()
        .filter(|finding| finding.severity == SupplyChainRiskSeverity::Warning)
        .count();
    let mut rule_ids = findings
        .iter()
        .map(|finding| finding.code.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    rule_ids.sort();
    SupplyChainRiskSummary {
        total_count: findings.len(),
        warning_count,
        info_count: findings.len().saturating_sub(warning_count),
        rule_ids,
    }
}

fn push_graph_finding(
    findings: &mut Vec<SupplyChainRiskFinding>,
    project: &ProjectMetadata,
    code: &str,
    severity: SupplyChainRiskSeverity,
    title: &str,
    description: &str,
    evidence: Vec<String>,
) {
    findings.push(SupplyChainRiskFinding {
        id: format!("{}:{}", project.path, code),
        code: code.into(),
        severity,
        title: title.into(),
        description: description.into(),
        project_path: project.path.clone(),
        node_id: None,
        dependency_path: Vec::new(),
        evidence,
    });
}

#[allow(clippy::too_many_arguments)]
fn push_node_finding(
    findings: &mut Vec<SupplyChainRiskFinding>,
    project: &ProjectMetadata,
    graph: &ProjectDependencyGraph,
    node: &DependencyGraphNode,
    code: &str,
    severity: SupplyChainRiskSeverity,
    title: &str,
    description: &str,
    evidence: Vec<String>,
) {
    findings.push(SupplyChainRiskFinding {
        id: format!("{}:{}:{}", project.path, code, node.id),
        code: code.into(),
        severity,
        title: title.into(),
        description: description.into(),
        project_path: project.path.clone(),
        node_id: Some(node.id.clone()),
        dependency_path: shortest_path(graph, &node.id),
        evidence,
    });
}

fn shortest_path(graph: &ProjectDependencyGraph, target_id: &str) -> Vec<String> {
    let Some(root) = graph
        .nodes
        .iter()
        .find(|node| node.kind == DependencyGraphNodeKind::Project)
    else {
        return Vec::new();
    };
    let mut adjacency = BTreeMap::<&str, Vec<&str>>::new();
    for edge in &graph.edges {
        adjacency
            .entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }
    let mut queue = VecDeque::from([(root.id.as_str(), vec![root.id.as_str()])]);
    let mut visited = BTreeSet::new();
    while let Some((current, path)) = queue.pop_front() {
        if !visited.insert(current) {
            continue;
        }
        if current == target_id {
            return path
                .into_iter()
                .filter_map(|id| graph.nodes.iter().find(|node| node.id == id))
                .map(|node| {
                    if node.kind == DependencyGraphNodeKind::Project {
                        node.name.clone()
                    } else {
                        format!("{}@{}", node.name, display_version(&node.version))
                    }
                })
                .collect();
        }
        for next in adjacency.get(current).into_iter().flatten() {
            let mut next_path = path.clone();
            next_path.push(next);
            queue.push_back((next, next_path));
        }
    }
    Vec::new()
}

fn display_version(version: &str) -> &str {
    if version.trim().is_empty() {
        "未解析"
    } else {
        version
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DependencyGraphEdge, DependencyGraphSummary};

    fn project() -> ProjectMetadata {
        ProjectMetadata {
            name: "web".into(),
            path: "/tmp/web".into(),
            ecosystems: vec!["JavaScript".into()],
            lock_files: vec!["package-lock.json".into()],
            runtime_requirements: vec![],
            package_manager: Some("npm".into()),
            dependencies: vec![],
            workspace: None,
            dependency_graph_summary: None,
            supply_chain_risk_summary: None,
            warnings: vec![],
        }
    }

    fn graph() -> ProjectDependencyGraph {
        let nodes = vec![
            DependencyGraphNode {
                id: "root".into(),
                ecosystem: "Project".into(),
                name: "web".into(),
                version: "".into(),
                kind: DependencyGraphNodeKind::Project,
                direct: false,
                scopes: vec![],
                package_url: None,
            },
            DependencyGraphNode {
                id: "react-19".into(),
                ecosystem: "JavaScript".into(),
                name: "react".into(),
                version: "19.1.1".into(),
                kind: DependencyGraphNodeKind::Package,
                direct: true,
                scopes: vec!["运行".into()],
                package_url: Some("pkg:npm/react@19.1.1".into()),
            },
            DependencyGraphNode {
                id: "scheduler".into(),
                ecosystem: "JavaScript".into(),
                name: "scheduler".into(),
                version: "0.26.0".into(),
                kind: DependencyGraphNodeKind::Package,
                direct: false,
                scopes: vec!["运行".into()],
                package_url: Some("pkg:npm/scheduler@0.26.0".into()),
            },
            DependencyGraphNode {
                id: "react-18".into(),
                ecosystem: "JavaScript".into(),
                name: "react".into(),
                version: "18.3.1".into(),
                kind: DependencyGraphNodeKind::Package,
                direct: true,
                scopes: vec!["开发".into()],
                package_url: Some("pkg:npm/react@18.3.1".into()),
            },
            DependencyGraphNode {
                id: "local".into(),
                ecosystem: "JavaScript".into(),
                name: "shared".into(),
                version: "workspace:*".into(),
                kind: DependencyGraphNodeKind::Local,
                direct: true,
                scopes: vec!["运行".into()],
                package_url: None,
            },
        ];
        let edges = vec![
            DependencyGraphEdge {
                from: "root".into(),
                to: "react-19".into(),
                dependency_type: "运行".into(),
            },
            DependencyGraphEdge {
                from: "react-19".into(),
                to: "scheduler".into(),
                dependency_type: "运行".into(),
            },
            DependencyGraphEdge {
                from: "root".into(),
                to: "react-18".into(),
                dependency_type: "开发".into(),
            },
            DependencyGraphEdge {
                from: "root".into(),
                to: "local".into(),
                dependency_type: "运行".into(),
            },
        ];
        ProjectDependencyGraph {
            project_name: "web".into(),
            project_path: "/tmp/web".into(),
            completeness: DependencyGraphCompleteness::Complete,
            sources: vec!["package-lock.json".into()],
            source_digest: "digest".into(),
            nodes,
            edges,
            warnings: vec![],
            summary: DependencyGraphSummary {
                node_count: 5,
                edge_count: 4,
                direct_count: 3,
                transitive_count: 1,
                duplicate_version_count: 1,
                unreachable_count: 0,
                cycle_count: 0,
                completeness: DependencyGraphCompleteness::Complete,
                sources: vec!["package-lock.json".into()],
                source_digest: "digest".into(),
            },
        }
    }

    #[test]
    fn reports_duplicate_versions_local_boundaries_and_paths() {
        let report = build_supply_chain_report(&project(), &graph());
        assert_eq!(report.summary.warning_count, 1);
        assert!(report
            .summary
            .rule_ids
            .contains(&"MULTIPLE_RESOLVED_VERSIONS".into()));
        let local = report
            .findings
            .iter()
            .find(|finding| finding.code == "NON_REGISTRY_DEPENDENCY")
            .unwrap();
        assert_eq!(local.dependency_path, vec!["web", "shared@workspace:*"]);
    }

    #[test]
    fn reports_graph_level_completeness_without_claiming_vulnerabilities() {
        let mut project = project();
        project.dependencies.push(crate::models::ProjectDependency {
            ecosystem: "JavaScript".into(),
            name: "react".into(),
            normalized_name: "react".into(),
            version_requirement: "^19".into(),
            scopes: vec!["运行".into()],
            resolved_version: None,
            resolution_source: None,
            resolution_checked: false,
        });
        let mut graph = graph();
        graph.completeness = DependencyGraphCompleteness::Unsupported;
        graph.summary.completeness = DependencyGraphCompleteness::Unsupported;
        let report = build_supply_chain_report(&project, &graph);
        assert!(report.summary.rule_ids.contains(&"LOCKFILE_MISSING".into()));
        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.code.contains("VULNERABILITY")));
    }
}
