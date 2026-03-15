use crate::app_settings::{get_backup_location, reset_backup_location, set_backup_location};
use crate::backup::{create_backup, delete_backup_file, import_backup_file, inspect_backup, list_backups};
use crate::models::{
    BackupCreateResponse, BackupInspectionResponse, BackupListItem, BackupLocationRequest,
    BackupLocationResponse, BackupOptions, DeleteBackupRequest, DetectInstallationResponse,
    ExecuteRestoreConfirmation, ExecuteRestoreResponse, ImportBackupResponse, OpenPathRequest,
    ProbeWsResponse, RestorePlan, RestorePlanOptions,
};
use crate::process::running_wavelink_processes;
use crate::restore::{execute_restore, force_close_wavelink, plan_restore, rollback_last_restore};
use crate::state::AppState;
use crate::wavelink_paths::{
    backup_folder_path, resolve_wavelink_local_state, settings_path, ws_info_path,
};
use crate::websocket_probe::probe_wave_link_ws;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::State;

#[tauri::command]
pub fn detect_wavelink_installation() -> Result<DetectInstallationResponse, String> {
    let local_state = resolve_wavelink_local_state(None);
    let (ws_info, settings, backup_dir, ws_port) = if let Some(local_state_path) = &local_state {
        let ws_info_path = ws_info_path(local_state_path);
        let settings_path = settings_path(local_state_path);
        let backup_path = backup_folder_path(local_state_path);
        let ws_port = fs::read_to_string(&ws_info_path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|json| json.get("port").and_then(|v| v.as_u64()))
            .map(|n| n as u16);
        (
            ws_info_path.to_string_lossy().to_string(),
            settings_path.to_string_lossy().to_string(),
            backup_path.to_string_lossy().to_string(),
            ws_port,
        )
    } else {
        ("".to_string(), "".to_string(), "".to_string(), None)
    };

    let process_names = running_wavelink_processes()?;
    let process_running = !process_names.is_empty();

    Ok(DetectInstallationResponse {
        local_state_path: local_state.map(|p| p.to_string_lossy().to_string()),
        ws_info_path: if ws_info.is_empty() {
            None
        } else {
            Some(ws_info)
        },
        ws_port,
        settings_path: if settings.is_empty() {
            None
        } else {
            Some(settings)
        },
        backup_dir: if backup_dir.is_empty() {
            None
        } else {
            Some(backup_dir)
        },
        process_running,
        process_names,
        platform: std::env::consts::OS.to_string(),
    })
}

#[tauri::command]
pub fn probe_wavelink_ws() -> Result<ProbeWsResponse, String> {
    let local_state =
        resolve_wavelink_local_state(None).ok_or("Wave Link LocalState path was not found")?;
    let ws_path = ws_info_path(&local_state);
    let text = fs::read_to_string(ws_path).map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let port = json
        .get("port")
        .and_then(|v| v.as_u64())
        .ok_or("Missing websocket port")? as u16;
    Ok(probe_wave_link_ws(port))
}

#[tauri::command]
pub fn create_backup_command(
    app_state: State<'_, AppState>,
    options: BackupOptions,
) -> Result<BackupCreateResponse, String> {
    let backup = create_backup(options)?;
    app_state.add_log(
        "backup",
        "info",
        format!("Created backup at {}", backup.backup_path),
        None,
    );
    Ok(backup)
}

#[tauri::command]
pub fn list_backups_command() -> Result<Vec<BackupListItem>, String> {
    list_backups()
}

#[tauri::command]
pub fn get_backup_location_command() -> Result<BackupLocationResponse, String> {
    get_backup_location()
}

#[tauri::command]
pub fn set_backup_location_command(
    request: BackupLocationRequest,
) -> Result<BackupLocationResponse, String> {
    set_backup_location(request)
}

#[tauri::command]
pub fn reset_backup_location_command() -> Result<BackupLocationResponse, String> {
    reset_backup_location()
}

#[tauri::command]
pub fn import_backup_command(
    app_state: State<'_, AppState>,
    source_path: String,
    overwrite: bool,
) -> Result<ImportBackupResponse, String> {
    let imported = import_backup_file(Path::new(&source_path), overwrite)?;
    app_state.add_log(
        "backup-import",
        "info",
        format!("Imported backup to {}", imported.backup_path),
        Some(serde_json::json!({ "overwritten": imported.overwritten })),
    );
    Ok(imported)
}

#[tauri::command]
pub fn delete_backup_command(
    app_state: State<'_, AppState>,
    request: DeleteBackupRequest,
) -> Result<(), String> {
    delete_backup_file(Path::new(&request.path))?;
    app_state.add_log(
        "backup-delete",
        "info",
        format!("Deleted backup {}", request.path),
        None,
    );
    Ok(())
}

#[tauri::command]
pub fn inspect_backup_command(path: String) -> Result<BackupInspectionResponse, String> {
    inspect_backup(Path::new(&path))
}

#[tauri::command]
pub fn plan_restore_command(
    app_state: State<'_, AppState>,
    path: String,
    options: Option<RestorePlanOptions>,
) -> Result<RestorePlan, String> {
    let plan = plan_restore(Path::new(&path), options.unwrap_or_default())?;
    {
        let mut plans = app_state
            .restore_plans
            .lock()
            .map_err(|_| "Lock poisoned")?;
        plans.insert(plan.plan_id.clone(), plan.clone());
    }
    app_state.add_log(
        "restore-plan",
        "info",
        format!("Planned restore from {}", path),
        Some(serde_json::json!({ "planId": plan.plan_id })),
    );
    Ok(plan)
}

#[tauri::command]
pub fn execute_restore_command(
    app_state: State<'_, AppState>,
    plan_id: String,
    confirmation: ExecuteRestoreConfirmation,
) -> Result<ExecuteRestoreResponse, String> {
    execute_restore(app_state, &plan_id, confirmation)
}

#[tauri::command]
pub fn rollback_last_restore_command(
    app_state: State<'_, AppState>,
) -> Result<ExecuteRestoreResponse, String> {
    rollback_last_restore(app_state)
}

#[tauri::command]
pub fn terminate_wavelink_processes_command() -> Result<Vec<String>, String> {
    force_close_wavelink()
}

#[tauri::command]
pub fn open_path_in_file_manager(request: OpenPathRequest) -> Result<(), String> {
    let path = PathBuf::from(request.path);
    if !path.exists() {
        return Err("Path does not exist".to_string());
    }

    if cfg!(target_os = "windows") {
        let status = Command::new("explorer")
            .arg(path.to_string_lossy().to_string())
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Failed to open explorer".to_string());
        }
        return Ok(());
    }

    if cfg!(target_os = "macos") {
        let status = Command::new("open")
            .arg(path.to_string_lossy().to_string())
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Failed to open path".to_string());
        }
        return Ok(());
    }

    Err("Unsupported platform".to_string())
}

#[tauri::command]
pub fn list_operation_logs(
    app_state: State<'_, AppState>,
) -> Result<Vec<crate::models::OperationLogEntry>, String> {
    let logs = app_state.logs.lock().map_err(|_| "Lock poisoned")?;
    Ok(logs.clone())
}
