use crate::models::{BackupLocationRequest, BackupLocationResponse};
use crate::wavelink_paths::default_backup_root;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PersistedAppSettings {
    #[serde(default)]
    backup_root: Option<String>,
}

pub fn get_backup_location() -> Result<BackupLocationResponse, String> {
    get_backup_location_from(&settings_file_path())
}

pub fn set_backup_location(request: BackupLocationRequest) -> Result<BackupLocationResponse, String> {
    set_backup_location_from(&settings_file_path(), request)
}

pub fn reset_backup_location() -> Result<BackupLocationResponse, String> {
    reset_backup_location_from(&settings_file_path())
}

pub fn managed_backup_root() -> Result<PathBuf, String> {
    let settings = load_settings(&settings_file_path())?;
    Ok(resolve_backup_root(&settings))
}

fn get_backup_location_from(settings_path: &Path) -> Result<BackupLocationResponse, String> {
    let settings = load_settings(settings_path)?;
    Ok(build_backup_location_response(&settings))
}

fn set_backup_location_from(
    settings_path: &Path,
    request: BackupLocationRequest,
) -> Result<BackupLocationResponse, String> {
    let trimmed = request.path.trim();
    if trimmed.is_empty() {
        return Err("Backup location cannot be empty".to_string());
    }

    let requested = PathBuf::from(trimmed);
    if requested.exists() && !requested.is_dir() {
        return Err("Backup location must be a folder".to_string());
    }

    fs::create_dir_all(&requested).map_err(|e| e.to_string())?;
    let normalized = requested.canonicalize().unwrap_or(requested);
    let normalized = sanitize_windows_path(&normalized);

    let mut settings = load_settings(settings_path)?;
    settings.backup_root = Some(normalized);
    save_settings(settings_path, &settings)?;

    Ok(build_backup_location_response(&settings))
}

fn reset_backup_location_from(settings_path: &Path) -> Result<BackupLocationResponse, String> {
    let mut settings = load_settings(settings_path)?;
    settings.backup_root = None;
    save_settings(settings_path, &settings)?;
    Ok(build_backup_location_response(&settings))
}

fn build_backup_location_response(settings: &PersistedAppSettings) -> BackupLocationResponse {
    let default_path = default_backup_root();
    let current_path = resolve_backup_root(settings);
    let is_custom = settings
        .backup_root
        .as_ref()
        .map(|path| PathBuf::from(path) != default_path)
        .unwrap_or(false);

    BackupLocationResponse {
        current_path: sanitize_windows_path_buf(&current_path),
        default_path: sanitize_windows_path_buf(&default_path),
        is_custom,
    }
}

fn resolve_backup_root(settings: &PersistedAppSettings) -> PathBuf {
    settings
        .backup_root
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(default_backup_root)
}

fn load_settings(settings_path: &Path) -> Result<PersistedAppSettings, String> {
    if !settings_path.exists() {
        return Ok(PersistedAppSettings::default());
    }

    let text = fs::read_to_string(settings_path).map_err(|e| e.to_string())?;
    serde_json::from_str::<PersistedAppSettings>(&text).map_err(|e| e.to_string())
}

fn save_settings(settings_path: &Path, settings: &PersistedAppSettings) -> Result<(), String> {
    if settings.backup_root.is_none() {
        if settings_path.exists() {
            fs::remove_file(settings_path).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }

    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let serialized = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(settings_path, serialized).map_err(|e| e.to_string())
}

fn settings_file_path() -> PathBuf {
    default_backup_root()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("settings.json")
}

fn sanitize_windows_path(path: &Path) -> String {
    sanitize_windows_path_buf(path)
}

fn sanitize_windows_path_buf(path: &Path) -> String {
    let value = path.to_string_lossy().to_string();
    if cfg!(target_os = "windows") {
        if let Some(stripped) = value.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{stripped}");
        }
        if let Some(stripped) = value.strip_prefix(r"\\?\") {
            return stripped.to_string();
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn returns_default_backup_location_when_no_settings_exist() {
        let temp = tempdir().expect("tempdir");
        let settings_path = temp.path().join("settings.json");

        let location = get_backup_location_from(&settings_path).expect("location");

        assert_eq!(location.current_path, default_backup_root().to_string_lossy());
        assert_eq!(location.default_path, default_backup_root().to_string_lossy());
        assert!(!location.is_custom);
    }

    #[test]
    fn stores_and_returns_custom_backup_location() {
        let temp = tempdir().expect("tempdir");
        let settings_path = temp.path().join("settings.json");
        let custom_backup_root = temp.path().join("custom-backups");

        let location = set_backup_location_from(
            &settings_path,
            BackupLocationRequest {
                path: custom_backup_root.to_string_lossy().to_string(),
            },
        )
        .expect("set location");

        assert_eq!(
            location.current_path,
            sanitize_windows_path_buf(&custom_backup_root.canonicalize().expect("canonical path"))
        );
        assert!(location.is_custom);
        assert!(settings_path.exists());
    }

    #[test]
    fn reset_removes_custom_backup_location_setting() {
        let temp = tempdir().expect("tempdir");
        let settings_path = temp.path().join("settings.json");
        let custom_backup_root = temp.path().join("custom-backups");

        set_backup_location_from(
            &settings_path,
            BackupLocationRequest {
                path: custom_backup_root.to_string_lossy().to_string(),
            },
        )
        .expect("set location");

        let location = reset_backup_location_from(&settings_path).expect("reset location");

        assert_eq!(location.current_path, default_backup_root().to_string_lossy());
        assert!(!location.is_custom);
        assert!(!settings_path.exists());
    }

    #[test]
    fn strips_windows_verbatim_prefix_from_display_path() {
        let raw = PathBuf::from(r"\\?\C:\Users\Mitchell\Documents");
        assert_eq!(
            sanitize_windows_path_buf(&raw),
            if cfg!(target_os = "windows") {
                r"C:\Users\Mitchell\Documents".to_string()
            } else {
                r"\\?\C:\Users\Mitchell\Documents".to_string()
            }
        );
    }
}
