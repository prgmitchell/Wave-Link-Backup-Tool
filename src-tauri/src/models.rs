use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectInstallationResponse {
    pub local_state_path: Option<String>,
    pub ws_info_path: Option<String>,
    pub ws_port: Option<u16>,
    pub settings_path: Option<String>,
    pub backup_dir: Option<String>,
    pub process_running: bool,
    pub process_names: Vec<String>,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeWsResponse {
    pub connected: bool,
    pub endpoint: Option<String>,
    pub app_info: Option<serde_json::Value>,
    pub mixes: Option<serde_json::Value>,
    pub channels: Option<serde_json::Value>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupOptions {
    pub output_dir: Option<String>,
    pub backup_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupCreateResponse {
    pub backup_path: String,
    pub manifest: BackupManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupListItem {
    pub path: String,
    pub display_name: String,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
    pub is_valid: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFileEntry {
    pub relative_path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub manifest_version: u32,
    pub tool_version: String,
    pub created_at: DateTime<Utc>,
    pub source_os: String,
    pub source_os_version: String,
    pub wave_link: Option<WaveLinkAppInfo>,
    pub local_state_relative_path: String,
    pub files: Vec<BackupFileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaveLinkAppInfo {
    pub name: Option<String>,
    pub app_id: Option<String>,
    pub version: Option<String>,
    pub build: Option<i64>,
    pub interface_revision: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInspectionResponse {
    pub backup_path: String,
    pub manifest: BackupManifest,
    pub valid_hashes: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RestorePlanOptions {
    pub user_mapping: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceMappingDecision {
    pub source_id: String,
    pub source_name: Option<String>,
    pub target_id: Option<String>,
    pub target_name: Option<String>,
    pub status: MappingStatus,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MappingStatus {
    Matched,
    AutoMapped,
    UserMapped,
    Unresolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePlan {
    pub plan_id: String,
    pub backup_path: String,
    pub generated_at: DateTime<Utc>,
    pub summary: RestorePlanSummary,
    pub mapping: Vec<DeviceMappingDecision>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePlanSummary {
    pub total_device_refs: usize,
    pub unresolved_count: usize,
    pub can_execute_without_force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteRestoreConfirmation {
    pub mapping_overrides: Option<HashMap<String, String>>,
    pub allow_unresolved: bool,
    pub launch_wavelink_after_restore: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteRestoreResponse {
    pub success: bool,
    pub message: String,
    pub rollback_backup_path: Option<String>,
    pub unresolved_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationLogEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub operation: String,
    pub level: String,
    pub message: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenPathRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBackupResponse {
    pub backup_path: String,
    pub overwritten: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteBackupRequest {
    pub path: String,
}
