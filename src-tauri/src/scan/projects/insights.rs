//! 跨项目直接依赖索引与版本分歧标记。
use std::collections::BTreeMap;

use crate::models::{DependencyInsight, DependencyProjectUsage, ProjectMetadata};

pub(super) fn dependency_insights(projects: &[ProjectMetadata]) -> Vec<DependencyInsight> {
    let mut insights = BTreeMap::<(String, String), DependencyInsight>::new();
    for project in projects {
        for dependency in &project.dependencies {
            let entry = insights
                .entry((
                    dependency.ecosystem.clone(),
                    dependency.normalized_name.clone(),
                ))
                .or_insert_with(|| DependencyInsight {
                    ecosystem: dependency.ecosystem.clone(),
                    name: dependency.name.clone(),
                    project_count: 0,
                    version_requirements: Vec::new(),
                    resolved_versions: Vec::new(),
                    projects: Vec::new(),
                    has_version_divergence: false,
                    has_resolved_version_divergence: false,
                    has_resolution_risk: false,
                    has_health_risk: false,
                });
            entry.project_count += 1;
            if !entry
                .version_requirements
                .contains(&dependency.version_requirement)
            {
                entry
                    .version_requirements
                    .push(dependency.version_requirement.clone());
            }
            entry.projects.push(DependencyProjectUsage {
                project_name: project.name.clone(),
                project_path: project.path.clone(),
                version_requirement: dependency.version_requirement.clone(),
                scopes: dependency.scopes.clone(),
                resolved_version: dependency.resolved_version.clone(),
                resolution_source: dependency.resolution_source.clone(),
            });
            if let Some(version) = &dependency.resolved_version {
                if !entry.resolved_versions.contains(version) {
                    entry.resolved_versions.push(version.clone());
                }
            }
            if dependency.resolution_checked && dependency.resolved_version.is_none() {
                entry.has_resolution_risk = true;
            }
        }
    }
    for insight in insights.values_mut() {
        insight.version_requirements.sort();
        insight.resolved_versions.sort();
        insight.has_version_divergence = insight.version_requirements.len() > 1;
        insight.has_resolved_version_divergence = insight.resolved_versions.len() > 1;
        insight.has_health_risk = insight.has_version_divergence
            || insight.has_resolved_version_divergence
            || insight.has_resolution_risk
            || insight
                .version_requirements
                .iter()
                .any(|requirement| is_dependency_risk(requirement));
        insight
            .projects
            .sort_by(|left, right| left.project_name.cmp(&right.project_name));
    }
    insights.into_values().collect()
}

fn is_dependency_risk(requirement: &str) -> bool {
    requirement == "未声明版本"
        || requirement.starts_with("workspace:")
        || requirement.starts_with("file:")
        || requirement.starts_with("link:")
        || requirement.starts_with("path:")
}
