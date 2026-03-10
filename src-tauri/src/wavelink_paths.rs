use std::env;
use std::path::{Path, PathBuf};

pub const WINDOWS_PACKAGE_PATH: &str = "Packages\\Elgato.WaveLink_g54w8ztgkx496\\LocalState";

pub fn default_backup_root() -> PathBuf {
    if cfg!(target_os = "windows") {
        if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
            return PathBuf::from(local_app_data)
                .join("Wave Link Backup Tool")
                .join("Backups");
        }
    }

    if let Some(home) = home_dir() {
        return home.join(".wavelink-backup-tool").join("backups");
    }

    PathBuf::from("./backups")
}

pub fn resolve_wavelink_local_state(override_path: Option<&str>) -> Option<PathBuf> {
    if let Some(custom) = override_path {
        let p = PathBuf::from(custom);
        if p.exists() {
            return Some(p);
        }
    }

    if cfg!(target_os = "windows") {
        return resolve_windows_local_state();
    }

    if cfg!(target_os = "macos") {
        return resolve_macos_local_state();
    }

    None
}

pub fn ws_info_path(local_state_path: &Path) -> PathBuf {
    local_state_path.join("ws-info.json")
}

pub fn settings_path(local_state_path: &Path) -> PathBuf {
    local_state_path.join("Settings.json")
}

pub fn backup_folder_path(local_state_path: &Path) -> PathBuf {
    local_state_path.join("Backup")
}

fn resolve_windows_local_state() -> Option<PathBuf> {
    let local_app_data = env::var("LOCALAPPDATA").ok()?;
    let path = PathBuf::from(local_app_data).join(WINDOWS_PACKAGE_PATH);
    path.exists().then_some(path)
}

fn resolve_macos_local_state() -> Option<PathBuf> {
    let home = home_dir()?;
    let candidates = vec![
        home.join("Library/Application Support/Elgato/Wave Link"),
        home.join("Library/Containers/com.elgato.WaveLink/Data/Library/Application Support/Elgato/Wave Link"),
        home.join("Library/Group Containers/group.com.elgato.wavelink/Wave Link"),
    ];

    candidates.into_iter().find(|candidate| candidate.exists())
}

fn home_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        env::var("USERPROFILE").ok().map(PathBuf::from)
    } else {
        env::var("HOME").ok().map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_existing_override_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let resolved = resolve_wavelink_local_state(Some(tmp.path().to_string_lossy().as_ref()));
        assert_eq!(resolved, Some(tmp.path().to_path_buf()));
    }
}
