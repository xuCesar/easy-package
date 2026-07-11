mod adapters;
mod commands;
#[cfg(feature = "e2e")]
mod e2e;
mod error;
mod models;
mod scan;
mod storage;

use commands::{
    add_scan_root, cancel_environment_scan, compare_snapshots, export_environment_report,
    export_snapshot_comparison_report, get_health_report, get_scan_logs, get_scan_settings,
    list_packages, list_projects, list_snapshot_summaries, remove_scan_root, scan_environment,
    update_scan_settings,
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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_environment,
            cancel_environment_scan,
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
        ])
        .run(tauri::generate_context!())
        .expect("Easy Package 启动失败");
}
