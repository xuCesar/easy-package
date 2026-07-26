//! 工作区发现：识别 JavaScript / Cargo 工作区并聚合成员。
use std::{fs, path::Path};

use serde_json::Value as JsonValue;
use toml::Value as TomlValue;

use crate::models::{ProjectMetadata, ProjectWorkspace, ProjectWorkspaceRef};

pub(super) fn discover_workspaces(projects: &mut [ProjectMetadata]) -> Vec<ProjectWorkspace> {
    let candidates = projects
        .iter()
        .flat_map(|project| {
            workspace_candidates(project)
                .into_iter()
                .map(move |candidate| (project.path.clone(), candidate))
        })
        .collect::<Vec<_>>();
    let mut workspaces = Vec::new();

    for (path, candidate) in candidates {
        let members = projects
            .iter()
            .filter(|project| {
                workspace_matches(&candidate, Path::new(&path), Path::new(&project.path))
            })
            .map(|project| project.path.clone())
            .collect::<Vec<_>>();
        if members.is_empty() {
            continue;
        }
        let workspace = ProjectWorkspace {
            name: candidate.name.clone(),
            path: path.clone(),
            ecosystem: candidate.ecosystem.clone(),
            member_paths: members.clone(),
        };
        for project in projects
            .iter_mut()
            .filter(|project| members.contains(&project.path))
        {
            project.workspace = Some(ProjectWorkspaceRef {
                name: workspace.name.clone(),
                path: workspace.path.clone(),
                ecosystem: workspace.ecosystem.clone(),
            });
        }
        workspaces.push(workspace);
    }
    workspaces.sort_by(|left, right| left.path.cmp(&right.path));
    workspaces
}

#[derive(Debug)]
struct WorkspaceCandidate {
    name: String,
    ecosystem: String,
    members: Vec<String>,
}

fn workspace_candidates(project: &ProjectMetadata) -> Vec<WorkspaceCandidate> {
    let directory = Path::new(&project.path);
    let mut candidates = Vec::new();
    if project
        .ecosystems
        .iter()
        .any(|ecosystem| ecosystem == "JavaScript")
    {
        if let Ok(source) = fs::read_to_string(directory.join("package.json")) {
            if let Ok(value) = serde_json::from_str::<JsonValue>(&source) {
                let members = match value.get("workspaces") {
                    Some(JsonValue::Array(items)) => items
                        .iter()
                        .filter_map(JsonValue::as_str)
                        .map(str::to_string)
                        .collect(),
                    Some(JsonValue::Object(value)) => value
                        .get("packages")
                        .and_then(JsonValue::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(JsonValue::as_str)
                        .map(str::to_string)
                        .collect(),
                    _ => Vec::new(),
                };
                if !members.is_empty() {
                    candidates.push(WorkspaceCandidate {
                        name: project.name.clone(),
                        ecosystem: "JavaScript".into(),
                        members,
                    });
                }
            }
        }
        if !candidates
            .iter()
            .any(|candidate| candidate.ecosystem == "JavaScript")
        {
            if let Ok(source) = fs::read_to_string(directory.join("pnpm-workspace.yaml")) {
                let members = source
                    .lines()
                    .map(str::trim)
                    .filter_map(|line| line.strip_prefix('-'))
                    .map(str::trim)
                    .map(|line| line.trim_matches(['\'', '"']).to_string())
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>();
                if !members.is_empty() {
                    candidates.push(WorkspaceCandidate {
                        name: project.name.clone(),
                        ecosystem: "JavaScript".into(),
                        members,
                    });
                }
            }
        }
    }
    if project
        .ecosystems
        .iter()
        .any(|ecosystem| ecosystem == "Rust")
    {
        if let Ok(source) = fs::read_to_string(directory.join("Cargo.toml")) {
            if let Ok(value) = source.parse::<TomlValue>() {
                let members = value
                    .get("workspace")
                    .and_then(|workspace| workspace.get("members"))
                    .and_then(TomlValue::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(TomlValue::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>();
                if !members.is_empty() {
                    candidates.push(WorkspaceCandidate {
                        name: project.name.clone(),
                        ecosystem: "Rust".into(),
                        members,
                    });
                }
            }
        }
    }
    candidates
}

fn workspace_matches(candidate: &WorkspaceCandidate, root: &Path, project: &Path) -> bool {
    let Ok(relative) = project.strip_prefix(root) else {
        return false;
    };
    let relative = relative.to_string_lossy().replace('\\', "/");
    candidate.members.iter().any(|pattern| {
        let normalized = pattern.trim_end_matches('/');
        if let Some(prefix) = normalized.strip_suffix("/*") {
            relative.starts_with(&format!("{prefix}/"))
                && !relative[prefix.len() + 1..].contains('/')
        } else {
            relative == normalized
        }
    })
}
