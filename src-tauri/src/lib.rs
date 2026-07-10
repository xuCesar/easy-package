mod adapters;
mod commands;
mod error;
mod models;
mod scan;
mod storage;

use commands::{
    add_scan_root, get_health_report, get_scan_logs, list_packages, list_projects,
    remove_scan_root, scan_environment,
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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_environment,
            list_packages,
            list_projects,
            add_scan_root,
            remove_scan_root,
            get_health_report,
            get_scan_logs,
        ])
        .run(tauri::generate_context!())
        .expect("Easy Package 启动失败");
}
