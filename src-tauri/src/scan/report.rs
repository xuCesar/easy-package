use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{error::AppError, models::EnvironmentScan};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportFormat {
    Json,
    Markdown,
}

impl ReportFormat {
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Json => "easy-package-report.json",
            Self::Markdown => "easy-package-report.md",
        }
    }

    pub fn filter_name(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Markdown => "Markdown",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Markdown => "md",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportExportResult {
    pub saved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

pub fn build_report(scan: &EnvironmentScan, format: ReportFormat) -> Result<String, AppError> {
    build_report_with_home(scan, format, dirs::home_dir().as_deref())
}

fn build_report_with_home(
    scan: &EnvironmentScan,
    format: ReportFormat,
    home: Option<&Path>,
) -> Result<String, AppError> {
    let mut report = serde_json::to_value(scan)?;
    let object = report
        .as_object_mut()
        .ok_or_else(|| AppError::Serialization("环境报告不是对象".into()))?;
    object.remove("logs");
    redact_value(&mut report, home);

    match format {
        ReportFormat::Json => Ok(serde_json::to_string_pretty(&report)?),
        ReportFormat::Markdown => Ok(markdown_report(&report)),
    }
}

fn redact_value(value: &mut Value, home: Option<&Path>) {
    match value {
        Value::String(content) => *content = redact_path(content, home),
        Value::Array(items) => items.iter_mut().for_each(|item| redact_value(item, home)),
        Value::Object(items) => items.values_mut().for_each(|item| redact_value(item, home)),
        _ => {}
    }
}

fn redact_path(value: &str, home: Option<&Path>) -> String {
    let Some(home) = home else {
        return value.into();
    };
    let home = home.to_string_lossy();
    if value == home {
        "~".into()
    } else if let Some(rest) = value.strip_prefix(home.as_ref()) {
        format!("~{rest}")
    } else {
        value.into()
    }
}

fn markdown_report(report: &Value) -> String {
    let scanned_at = report["scannedAt"].as_str().unwrap_or("未知");
    let roots = report["scanRoots"]
        .as_array()
        .map(|roots| {
            roots
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("、")
        })
        .filter(|roots| !roots.is_empty())
        .unwrap_or_else(|| "未配置".into());
    let managers = report["managers"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let packages = report["packages"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let projects = report["projects"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let issues = report["healthIssues"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();

    let mut output = format!(
        "# Easy Package 环境报告\n\n- 扫描时间：{scanned_at}\n- 扫描目录：{roots}\n- 只读模式：是\n\n## 包管理器\n\n| 管理器 | 版本 | 状态 | 路径 |\n| --- | --- | --- | --- |\n"
    );
    for manager in managers {
        output.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            manager["displayName"].as_str().unwrap_or("—"),
            manager["version"].as_str().unwrap_or("—"),
            manager["status"].as_str().unwrap_or("—"),
            manager["executablePath"].as_str().unwrap_or("—")
        ));
    }
    output.push_str(&format!(
        "\n## 软件包\n\n共 {} 个已发现软件包。\n\n## 项目\n\n",
        packages.len()
    ));
    for project in projects {
        output.push_str(&format!(
            "- **{}** — `{}`\n",
            project["name"].as_str().unwrap_or("未命名项目"),
            project["path"].as_str().unwrap_or("—")
        ));
    }
    output.push_str("\n## 健康提示\n\n");
    if issues.is_empty() {
        output.push_str("未发现需要关注的问题。\n");
    } else {
        for issue in issues {
            output.push_str(&format!(
                "- **{}**：{}\n",
                issue["title"].as_str().unwrap_or("健康提示"),
                issue["description"].as_str().unwrap_or("—")
            ));
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;

    use super::*;

    fn scan() -> EnvironmentScan {
        serde_json::from_value(json!({
            "managers": [{"id":"npm","displayName":"npm","version":"11","executablePath":"/Users/demo/.local/bin/npm","status":"available","capabilities":[],"scannedAt":"now"}],
            "packages": [], "projects": [{"name":"app","path":"/Users/demo/Code/app","ecosystems":[],"lockFiles":[],"runtimeRequirements":[],"warnings":[]}],
            "scanRoots":["/Users/demo/Code"], "healthIssues":[], "logs":[{"id":"secret-log","category":"scan","status":"info","message":"diagnostic output","output":"should not export","timestamp":"now"}], "pathObservations":[], "scannedAt":"now", "partialFailures":0
        })).unwrap()
    }

    #[test]
    fn json_report_redacts_home_paths_and_excludes_logs() {
        let report =
            build_report_with_home(&scan(), ReportFormat::Json, Some(Path::new("/Users/demo")))
                .unwrap();
        assert!(serde_json::from_str::<Value>(&report).is_ok());
        assert!(report.contains("~/Code/app"));
        assert!(!report.contains("/Users/demo"));
        assert!(!report.contains("\"logs\""));
        assert!(!report.contains("diagnostic output"));
        assert!(!report.contains("should not export"));
    }

    #[test]
    fn markdown_report_has_stable_sections() {
        let report = build_report_with_home(
            &scan(),
            ReportFormat::Markdown,
            Some(Path::new("/Users/demo")),
        )
        .unwrap();
        assert!(report.contains("# Easy Package 环境报告"));
        assert!(report.contains("## 包管理器"));
        assert!(report.contains("## 项目"));
        assert!(report.contains("~/Code/app"));
    }
}
