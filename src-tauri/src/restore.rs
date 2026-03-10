use crate::backup::{create_backup, extract_backup_to_dir, inspect_backup, read_live_channels_snapshot};
use crate::models::{
    BackupOptions, DeviceMappingDecision, ExecuteRestoreConfirmation, ExecuteRestoreResponse,
    MappingStatus, RestorePlan, RestorePlanOptions, RestorePlanSummary,
};
use crate::process::{
    filter_blocking_processes, launch_wavelink, running_wavelink_processes, terminate_wavelink_processes,
};
use crate::state::AppState;
use crate::websocket_probe::apply_channel_levels;
use crate::wavelink_paths::{resolve_wavelink_local_state, settings_path};
use chrono::Utc;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::Path;
use std::thread;
use std::time::Duration;
use tauri::State;
use tempfile::tempdir;
use uuid::Uuid;

pub fn plan_restore(path: &Path, options: RestorePlanOptions) -> Result<RestorePlan, String> {
    let inspection = inspect_backup(path)?;
    if !inspection.valid_hashes {
        return Err("Backup archive failed checksum verification".to_string());
    }

    let local_state =
        resolve_wavelink_local_state(None).ok_or("Wave Link LocalState path was not found")?;
    let current_settings = settings_path(&local_state);
    let current_json = if current_settings.exists() {
        fs::read_to_string(current_settings)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    } else {
        None
    };

    let temp = tempdir().map_err(|e| e.to_string())?;
    let extracted_manifest = extract_backup_to_dir(path, temp.path())?;
    let extracted_settings = temp.path().join("Settings.json");
    let backup_json = if extracted_settings.exists() {
        fs::read_to_string(extracted_settings)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    } else {
        None
    };

    let user_mapping = options.user_mapping.unwrap_or_default();
    let mapping = build_mapping_plan(backup_json.as_ref(), current_json.as_ref(), &user_mapping);
    let unresolved_count = mapping
        .iter()
        .filter(|m| matches!(m.status, MappingStatus::Unresolved))
        .count();

    let mut warnings = inspection.warnings;
    if unresolved_count > 0 {
        warnings.push(format!(
            "{unresolved_count} device references are unresolved and may require manual mapping"
        ));
    }
    if extracted_manifest.source_os != std::env::consts::OS {
        warnings.push(format!(
            "Backup source OS is '{}', current OS is '{}'",
            extracted_manifest.source_os,
            std::env::consts::OS
        ));
    }

    Ok(RestorePlan {
        plan_id: Uuid::new_v4().to_string(),
        backup_path: path.to_string_lossy().to_string(),
        generated_at: Utc::now(),
        summary: RestorePlanSummary {
            total_device_refs: mapping.len(),
            unresolved_count,
            can_execute_without_force: unresolved_count == 0,
        },
        mapping,
        warnings,
    })
}

pub fn execute_restore(
    app_state: State<'_, AppState>,
    plan_id: &str,
    confirmation: ExecuteRestoreConfirmation,
) -> Result<ExecuteRestoreResponse, String> {
    let plan = {
        let plans = app_state
            .restore_plans
            .lock()
            .map_err(|_| "Lock poisoned")?;
        plans
            .get(plan_id)
            .cloned()
            .ok_or("Restore plan not found")?
    };

    let unresolved_count = plan
        .mapping
        .iter()
        .filter(|m| matches!(m.status, MappingStatus::Unresolved))
        .count();

    if unresolved_count > 0 && !confirmation.allow_unresolved {
        return Err(
            "Restore plan has unresolved mappings; enable allowUnresolved to continue".to_string(),
        );
    }

    let running = running_wavelink_processes()?;
    let blocking = filter_blocking_processes(&running);
    if !blocking.is_empty() {
        return Err(format!(
            "Wave Link must be closed before restore. Running processes: {}",
            blocking.join(", ")
        ));
    }

    let local_state =
        resolve_wavelink_local_state(None).ok_or("Wave Link LocalState path was not found")?;
    let rollback = create_backup(BackupOptions {
        output_dir: None,
        backup_name: Some(format!("pre-restore-{}.wlbk", Uuid::new_v4())),
    })?;

    let mapping = confirmation.mapping_overrides.unwrap_or_default();
    let temp = tempdir().map_err(|e| e.to_string())?;
    let _ = extract_backup_to_dir(Path::new(&plan.backup_path), temp.path())?;

    let backup_settings = temp.path().join("Settings.json");
    if backup_settings.exists() && !mapping.is_empty() {
        apply_mapping_to_settings_file(&backup_settings, &mapping)?;
    }

    apply_directory_replace(temp.path(), &local_state)?;

    if confirmation.launch_wavelink_after_restore {
        let _ = launch_wavelink();
    }

    // Some top-level channel fader values are runtime values exposed via websocket and may not
    // be persisted in Settings.json. Re-apply from backup snapshot briefly after restore.
    // Keep this retry window short so the user can immediately take manual control.
    let mut live_channel_levels_applied = 0usize;
    if let Some(snapshot) = read_live_channels_snapshot(Path::new(&plan.backup_path))? {
        live_channel_levels_applied = reapply_live_channel_levels_with_retries(
            &snapshot,
            if confirmation.launch_wavelink_after_restore {
                6
            } else {
                2
            },
            450,
        );
    }

    {
        let mut last = app_state
            .last_rollback_backup
            .lock()
            .map_err(|_| "Lock poisoned")?;
        *last = Some(rollback.backup_path.clone());
    }

    app_state.add_log(
        "restore",
        "info",
        "Restore executed successfully",
        Some(serde_json::json!({
            "planId": plan_id,
            "rollbackBackupPath": rollback.backup_path,
            "unresolvedCount": unresolved_count,
            "liveChannelLevelsApplied": live_channel_levels_applied
        })),
    );

    Ok(ExecuteRestoreResponse {
        success: true,
        message: "Restore completed successfully".to_string(),
        rollback_backup_path: Some(rollback.backup_path),
        unresolved_count,
    })
}

pub fn rollback_last_restore(
    app_state: State<'_, AppState>,
) -> Result<ExecuteRestoreResponse, String> {
    let rollback_path = {
        let guard = app_state
            .last_rollback_backup
            .lock()
            .map_err(|_| "Lock poisoned")?;
        guard.clone().ok_or("No rollback backup available")?
    };

    let plan = plan_restore(Path::new(&rollback_path), RestorePlanOptions::default())?;
    {
        let mut plans = app_state
            .restore_plans
            .lock()
            .map_err(|_| "Lock poisoned")?;
        plans.insert(plan.plan_id.clone(), plan.clone());
    }

    execute_restore(
        app_state,
        &plan.plan_id,
        ExecuteRestoreConfirmation {
            allow_unresolved: true,
            launch_wavelink_after_restore: true,
            mapping_overrides: None,
        },
    )
}

pub fn force_close_wavelink() -> Result<Vec<String>, String> {
    terminate_wavelink_processes()
}

fn wait_for_ws_port(max_attempts: usize, delay_ms: u64) -> Option<u16> {
    for _ in 0..max_attempts {
        let Some(local_state) = resolve_wavelink_local_state(None) else {
            thread::sleep(Duration::from_millis(delay_ms));
            continue;
        };
        let ws_info = local_state.join("ws-info.json");
        let content = match fs::read_to_string(ws_info) {
            Ok(v) => v,
            Err(_) => {
                thread::sleep(Duration::from_millis(delay_ms));
                continue;
            }
        };
        let parsed = serde_json::from_str::<serde_json::Value>(&content).ok();
        if let Some(port) = parsed
            .as_ref()
            .and_then(|v| v.get("port"))
            .and_then(|v| v.as_u64())
        {
            return Some(port as u16);
        }
        thread::sleep(Duration::from_millis(delay_ms));
    }
    None
}

fn reapply_live_channel_levels_with_retries(
    snapshot: &serde_json::Value,
    attempts: usize,
    delay_ms: u64,
) -> usize {
    let mut last_applied = 0usize;
    for _ in 0..attempts {
        if let Some(port) = wait_for_ws_port(1, 0) {
            if let Ok(applied) = apply_channel_levels(port, snapshot) {
                last_applied = applied;
            }
        }
        thread::sleep(Duration::from_millis(delay_ms));
    }
    last_applied
}

fn apply_directory_replace(source_dir: &Path, target_dir: &Path) -> Result<(), String> {
    if !target_dir.exists() {
        fs::create_dir_all(target_dir).map_err(|e| e.to_string())?;
    }

    for entry in walkdir::WalkDir::new(source_dir).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().is_file() {
            continue;
        }

        let rel = entry
            .path()
            .strip_prefix(source_dir)
            .map_err(|e| e.to_string())?;
        let target_file = target_dir.join(rel);
        if let Some(parent) = target_file.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let tmp_target = target_file.with_extension("tmp_restore");
        fs::copy(entry.path(), &tmp_target).map_err(|e| e.to_string())?;
        fs::rename(tmp_target, target_file).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn apply_mapping_to_settings_file(
    path: &Path,
    mapping: &HashMap<String, String>,
) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    remap_json_values(&mut json, mapping);
    let out = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
    fs::write(path, out).map_err(|e| e.to_string())
}

fn remap_json_values(value: &mut serde_json::Value, mapping: &HashMap<String, String>) {
    match value {
        serde_json::Value::String(s) => {
            if let Some(mapped) = mapping.get(s) {
                *s = mapped.clone();
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                remap_json_values(item, mapping);
            }
        }
        serde_json::Value::Object(map) => {
            let old_keys: Vec<String> = map.keys().cloned().collect();
            for key in old_keys {
                if let Some(mut value) = map.remove(&key) {
                    remap_json_values(&mut value, mapping);
                    let new_key = mapping.get(&key).cloned().unwrap_or(key);
                    map.insert(new_key, value);
                }
            }
        }
        _ => {}
    }
}

fn build_mapping_plan(
    backup_json: Option<&serde_json::Value>,
    current_json: Option<&serde_json::Value>,
    user_mapping: &HashMap<String, String>,
) -> Vec<DeviceMappingDecision> {
    let backup_devices = extract_device_map(backup_json);
    let current_devices = extract_device_map(current_json);

    let mut mapping = Vec::new();
    for (source_id, source_name) in backup_devices {
        if let Some(target_id) = user_mapping.get(&source_id) {
            mapping.push(DeviceMappingDecision {
                source_id: source_id.clone(),
                source_name: source_name.clone(),
                target_id: Some(target_id.clone()),
                target_name: current_devices.get(target_id).cloned().flatten(),
                status: MappingStatus::UserMapped,
                reason: "Provided by user mapping".to_string(),
            });
            continue;
        }

        if current_devices.contains_key(&source_id) {
            mapping.push(DeviceMappingDecision {
                source_id: source_id.clone(),
                source_name: source_name.clone(),
                target_id: Some(source_id.clone()),
                target_name: current_devices.get(&source_id).cloned().flatten(),
                status: MappingStatus::Matched,
                reason: "Exact device id exists".to_string(),
            });
            continue;
        }

        let auto_target = source_name.as_ref().and_then(|name| {
            current_devices
                .iter()
                .find(|(_, n)| {
                    n.as_ref()
                        .map(|v| v.eq_ignore_ascii_case(name))
                        .unwrap_or(false)
                })
                .map(|(id, _)| id.clone())
        });

        if let Some(target_id) = auto_target {
            mapping.push(DeviceMappingDecision {
                source_id: source_id.clone(),
                source_name: source_name.clone(),
                target_id: Some(target_id.clone()),
                target_name: current_devices.get(&target_id).cloned().flatten(),
                status: MappingStatus::AutoMapped,
                reason: "Name-based auto mapping".to_string(),
            });
        } else {
            mapping.push(DeviceMappingDecision {
                source_id,
                source_name,
                target_id: None,
                target_name: None,
                status: MappingStatus::Unresolved,
                reason: "No exact or name match in current settings".to_string(),
            });
        }
    }

    mapping.sort_by(|a, b| a.source_id.cmp(&b.source_id));
    mapping
}

fn extract_device_map(input: Option<&serde_json::Value>) -> HashMap<String, Option<String>> {
    let mut ids: BTreeSet<String> = BTreeSet::new();
    let mut names: HashMap<String, Option<String>> = HashMap::new();
    let Some(root) = input else {
        return names;
    };

    let input_settings = root
        .get("MixerConfiguration")
        .and_then(|m| m.get("InputSettings"))
        .and_then(|v| v.as_object());

    if let Some(settings) = input_settings {
        for (id, cfg) in settings {
            ids.insert(id.clone());
            let name = cfg
                .get("InputName")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            names.insert(id.clone(), name);
        }
    }

    for id in ids {
        names.entry(id).or_insert(None);
    }

    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaps_object_keys_and_values() {
        let mut json = serde_json::json!({
            "MixerConfiguration": {
                "InputSettings": {
                    "OLD_ID": {
                        "InputName": "Mic",
                        "PipelineDeviceInternalId": "OLD_ID"
                    }
                }
            }
        });
        let mut map = HashMap::new();
        map.insert("OLD_ID".to_string(), "NEW_ID".to_string());

        remap_json_values(&mut json, &map);
        let input_settings = &json["MixerConfiguration"]["InputSettings"];
        assert!(input_settings.get("OLD_ID").is_none());
        assert!(input_settings.get("NEW_ID").is_some());
        assert_eq!(
            json["MixerConfiguration"]["InputSettings"]["NEW_ID"]["PipelineDeviceInternalId"],
            "NEW_ID"
        );
    }

    #[test]
    fn build_mapping_marks_unresolved_when_missing() {
        let backup = serde_json::json!({
            "MixerConfiguration": {
                "InputSettings": {
                    "SRC_A": { "InputName": "Game" }
                }
            }
        });
        let current = serde_json::json!({
            "MixerConfiguration": {
                "InputSettings": {
                    "DST_B": { "InputName": "Browser" }
                }
            }
        });

        let mapping = build_mapping_plan(Some(&backup), Some(&current), &HashMap::new());
        assert_eq!(mapping.len(), 1);
        assert!(matches!(mapping[0].status, MappingStatus::Unresolved));
    }
}
