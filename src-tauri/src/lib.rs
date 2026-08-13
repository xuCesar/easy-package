mod actions;
mod adapters;
mod bindings;
mod commands;
#[cfg(feature = "e2e")]
mod e2e;
mod error;
mod models;
mod operation_guard;
mod scan;
mod services;
mod storage;

use commands::{
    add_scan_root, cancel_environment_scan, cancel_package_action, cancel_package_catalog_search,
    compare_snapshots, execute_package_action, export_environment_report, export_project_sbom,
    export_snapshot_comparison_report, get_health_report, get_latest_snapshot,
    get_package_action_capabilities, get_project_dependency_graph, get_project_supply_chain_report,
    get_scan_logs, get_scan_settings, list_package_action_audit, list_packages, list_projects,
    list_snapshot_summaries, plan_package_action, reconcile_package_action, remove_scan_root,
    scan_environment, search_package_catalog, update_scan_settings,
};
use storage::Storage;
use tauri::Manager;

/// 日志默认 info 级别，可用 EASY_PACKAGE_LOG 覆盖（如 devpkg_lib=debug）。
/// dev 构建输出可读格式，release 构建输出 JSON 行。
fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_env("EASY_PACKAGE_LOG")
        .unwrap_or_else(|_| EnvFilter::new("devpkg_lib=info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    #[cfg(debug_assertions)]
    builder.init();
    #[cfg(not(debug_assertions))]
    builder.json().init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let storage = Storage::new(app.handle())?;
            // 崩溃后遗留的 Running 审计立即转入待核对状态，而不是等下一次扫描。
            match storage.recover_incomplete_actions() {
                Ok(recovered) if recovered > 0 => {
                    tracing::info!(recovered, "启动时将中断的写操作标记为待核对");
                }
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "启动时恢复未完成写操作失败"),
            }
            app.manage(storage);
            app.manage(commands::ScanRegistry::default());
            app.manage(scan::dependency_graph::DependencyGraphCache::default());
            app.manage(actions::ActionRegistry::default());
            app.manage(actions::catalog::CatalogRegistry::default());
            app.manage(operation_guard::OperationCoordinator::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_latest_snapshot,
            scan_environment,
            cancel_environment_scan,
            search_package_catalog,
            cancel_package_catalog_search,
            list_packages,
            list_projects,
            add_scan_root,
            remove_scan_root,
            get_scan_settings,
            update_scan_settings,
            export_environment_report,
            list_snapshot_summaries,
            compare_snapshots,
            export_snapshot_comparison_report,
            get_health_report,
            get_scan_logs,
            get_project_dependency_graph,
            get_project_supply_chain_report,
            export_project_sbom,
            plan_package_action,
            get_package_action_capabilities,
            execute_package_action,
            cancel_package_action,
            list_package_action_audit,
            reconcile_package_action,
        ])
        .run(tauri::generate_context!())
        .expect("Easy Package 启动失败");
}
