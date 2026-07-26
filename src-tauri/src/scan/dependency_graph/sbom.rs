//! CycloneDX 1.6 SBOM 导出：不包含本机路径，仅附带离线风险摘要。
use super::*;
#[cfg(test)]
pub(super) fn build_cyclonedx_sbom(graph: &ProjectDependencyGraph) -> Result<String, AppError> {
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
