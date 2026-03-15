mod app_settings;
mod backup;
mod commands;
mod models;
mod process;
mod restore;
mod state;
mod wavelink_paths;
mod websocket_probe;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .setup(|app| {
            app.handle().plugin(tauri_plugin_dialog::init())?;
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::detect_wavelink_installation,
            commands::probe_wavelink_ws,
            commands::create_backup_command,
            commands::list_backups_command,
            commands::get_backup_location_command,
            commands::set_backup_location_command,
            commands::reset_backup_location_command,
            commands::import_backup_command,
            commands::delete_backup_command,
            commands::inspect_backup_command,
            commands::plan_restore_command,
            commands::execute_restore_command,
            commands::rollback_last_restore_command,
            commands::terminate_wavelink_processes_command,
            commands::open_path_in_file_manager,
            commands::list_operation_logs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
