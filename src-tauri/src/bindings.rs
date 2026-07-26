//! 前端 TS 类型由 Rust models 生成：
//! 校验：`cargo test bindings`（types.gen.ts 与 models 不一致时失败）
//! 重新生成：`EASY_PACKAGE_EXPORT_TYPES=1 cargo test bindings`
#![cfg(test)]

use specta::TypeCollection;
use specta_typescript::{BigIntExportBehavior, Typescript};

use crate::{models, scan::report};

fn generate_typescript() -> String {
    let mut types = TypeCollection::default();
    types
        .register::<models::PackageManagerId>()
        .register::<models::ManagerStatus>()
        .register::<models::ExecutionTrust>()
        .register::<models::CacheScanStatus>()
        .register::<models::DiagnosticError>()
        .register::<models::PackageManager>()
        .register::<models::PackageScope>()
        .register::<models::UpdateStatus>()
        .register::<models::ManagedPackage>()
        .register::<models::CatalogSearchStatus>()
        .register::<models::CatalogSearchBlockerCode>()
        .register::<models::CatalogSearchResult>()
        .register::<models::CatalogSearchResponse>()
        .register::<models::RuntimeRequirement>()
        .register::<models::ProjectMetadata>()
        .register::<models::ProjectWorkspaceRef>()
        .register::<models::ProjectWorkspace>()
        .register::<models::NetworkPolicy>()
        .register::<models::ScanSettings>()
        .register::<models::RuntimeInstallation>()
        .register::<models::RuntimeRequirementStatus>()
        .register::<models::RuntimeRequirementAssessment>()
        .register::<models::ProjectDependency>()
        .register::<models::DependencyGraphCompleteness>()
        .register::<models::DependencyGraphNodeKind>()
        .register::<models::DependencyGraphNode>()
        .register::<models::DependencyGraphEdge>()
        .register::<models::DependencyGraphSummary>()
        .register::<models::ProjectDependencyGraph>()
        .register::<models::SupplyChainRiskSeverity>()
        .register::<models::SupplyChainRiskFinding>()
        .register::<models::SupplyChainRiskSummary>()
        .register::<models::ProjectSupplyChainReport>()
        .register::<models::DependencyProjectUsage>()
        .register::<models::DependencyInsight>()
        .register::<models::ProjectAnalysis>()
        .register::<models::HealthSeverity>()
        .register::<models::HealthIssue>()
        .register::<models::LogCategory>()
        .register::<models::LogStatus>()
        .register::<models::TaskLog>()
        .register::<models::PathObservation>()
        .register::<models::PathCandidate>()
        .register::<models::EnvironmentScan>()
        .register::<models::SnapshotSummary>()
        .register::<models::SnapshotChangeKind>()
        .register::<models::SnapshotChangeEntity>()
        .register::<models::SnapshotChange>()
        .register::<models::SnapshotComparison>()
        .register::<models::PackageAction>()
        .register::<models::PackageActionStatus>()
        .register::<models::ActionCheckStatus>()
        .register::<models::ActionBlockerCode>()
        .register::<models::ActionPreflightCheck>()
        .register::<models::ActionCapability>()
        .register::<models::ObservedActionOutcome>()
        .register::<models::PackageActionPlan>()
        .register::<models::PackageActionProgress>()
        .register::<models::PackageActionResult>()
        .register::<models::PackageActionAuditRecord>()
        .register::<models::PackageActionReconciliationResult>()
        .register::<models::ScanPhase>()
        .register::<models::ScanProgress>()
        .register::<report::ReportFormat>()
        .register::<report::ReportExportResult>();
    Typescript::default()
        .bigint(BigIntExportBehavior::Number)
        .header("// 本文件由 src-tauri 的 bindings 测试从 Rust models 生成，请勿手工修改。\n// 重新生成：EASY_PACKAGE_EXPORT_TYPES=1 cargo test bindings\n")
        .export(&types)
        .map(normalize_output_contract)
        .expect("导出 TypeScript 类型失败")
}

/// specta 按「可反序列化」视角生成：serde(default) 字段变可选、Option+skip 字段带 `| null`。
/// 前端只消费序列化输出：default 字段总会输出（转必填），Option+skip 字段缺席而非 null。
fn normalize_output_contract(generated: String) -> String {
    let mut result = String::with_capacity(generated.len());
    for line in generated.lines() {
        let mut line = line.to_string();
        if line.starts_with("export type") {
            let mut rebuilt = String::with_capacity(line.len());
            let mut rest = line.as_str();
            while let Some(index) = rest.find("?: ") {
                let (head, tail) = rest.split_at(index);
                rebuilt.push_str(head);
                let value_end = tail.find([';', '}']).unwrap_or(tail.len());
                let value = tail[3..value_end].trim_end();
                let trailing = &tail[3 + value.len()..value_end];
                if let Some(stripped) = value.strip_suffix(" | null") {
                    rebuilt.push_str("?: ");
                    rebuilt.push_str(stripped);
                } else {
                    rebuilt.push_str(": ");
                    rebuilt.push_str(value);
                }
                rebuilt.push_str(trailing);
                rest = &tail[value_end..];
            }
            rebuilt.push_str(rest);
            line = rebuilt;
        }
        result.push_str(&line);
        result.push('\n');
    }
    result
}

#[test]
fn generated_typescript_matches_models() {
    let generated = generate_typescript();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/types.gen.ts");
    if std::env::var("EASY_PACKAGE_EXPORT_TYPES").as_deref() == Ok("1") {
        std::fs::write(&path, &generated).expect("写入 types.gen.ts 失败");
        return;
    }
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        existing, generated,
        "src/types.gen.ts 与 Rust models 不一致；运行 EASY_PACKAGE_EXPORT_TYPES=1 cargo test bindings 重新生成"
    );
}
