use crate::models::{OperationLogEntry, RestorePlan};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub struct AppState {
    pub restore_plans: Mutex<HashMap<String, RestorePlan>>,
    pub last_rollback_backup: Mutex<Option<String>>,
    pub logs: Mutex<Vec<OperationLogEntry>>,
}

impl AppState {
    pub fn add_log(
        &self,
        operation: impl Into<String>,
        level: impl Into<String>,
        message: impl Into<String>,
        metadata: Option<serde_json::Value>,
    ) {
        if let Ok(mut logs) = self.logs.lock() {
            logs.push(OperationLogEntry {
                id: Uuid::new_v4().to_string(),
                timestamp: Utc::now(),
                operation: operation.into(),
                level: level.into(),
                message: message.into(),
                metadata,
            });
            if logs.len() > 500 {
                let trim = logs.len() - 500;
                logs.drain(0..trim);
            }
        }
    }
}
