mod actions;
mod adapters;
mod commands;
#[cfg(feature = "e2e")]
mod e2e;
mod error;
mod models;
mod operation_guard;
mod scan;
mod storage;

use commands::{
    add_scan_root, cancel_environment_scan, cancel_package_action, cancel_package_catalog_search,
    compare_snapshots, execute_package_action, export_environment_report, export_project_sbom,
    export_snapshot_comparison_report, get_health_report, get_package_action_capabilities,
    get_project_dependency_graph, get_project_supply_chain_report, get_scan_logs,
    get_scan_settings, list_package_action_audit, list_packages, list_projects,
    list_snapshot_summaries, plan_package_action, reconcile_package_action, remove_scan_root,
    scan_environment, search_package_catalog, update_scan_settings,
};
use storage::Storage;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let storage = Storage::new(app.handle())?;
            app.manage(storage);
            app.manage(commands::ScanRegistry::default());
            app.manage(actions::ActionRegistry::default());
            app.manage(actions::catalog::CatalogRegistry::default());
            app.manage(operation_guard::OperationCoordinator::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
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
