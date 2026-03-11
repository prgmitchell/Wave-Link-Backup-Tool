use crate::models::{
    BackupCreateResponse, BackupFileEntry, BackupInspectionResponse, BackupListItem,
    BackupManifest, BackupOptions, ImportBackupResponse,
};
use crate::wavelink_paths::{
    default_backup_root, resolve_wavelink_local_state, settings_path, ws_info_path,
};
use crate::websocket_probe::{app_info_from_probe, probe_wave_link_ws};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;
use tempfile::tempdir;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

pub fn create_backup(options: BackupOptions) -> Result<BackupCreateResponse, String> {
    let local_state =
        resolve_wavelink_local_state(None).ok_or("Wave Link LocalState path was not found")?;
    create_backup_from_local_state(options, &local_state)
}

fn create_backup_from_local_state(
    options: BackupOptions,
    local_state: &Path,
) -> Result<BackupCreateResponse, String> {
    let snapshot = snapshot_local_state(local_state)?;

    let output_dir = options
        .output_dir
        .map(PathBuf::from)
        .unwrap_or_else(default_backup_root);
    fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

    let now = Utc::now();
    let backup_name = options
        .backup_name
        .unwrap_or_else(|| format!("wavelink-backup-{}.wlbk", now.format("%Y%m%d-%H%M%S")));
    let backup_path = output_dir.join(ensure_backup_extension(&backup_name));

    let ws_port = read_ws_port(&local_state);
    let probe = ws_port.map(probe_wave_link_ws);
    let wave_link_info = probe
        .as_ref()
        .and_then(|p| app_info_from_probe(p.app_info.as_ref()));

    let mut files = collect_files(snapshot.path())?;
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut entries = Vec::with_capacity(files.len());
    for (rel, abs) in &files {
        let metadata = fs::metadata(abs).map_err(|e| e.to_string())?;
        let sha256 = file_sha256(abs)?;
        entries.push(BackupFileEntry {
            relative_path: rel.clone(),
            size: metadata.len(),
            sha256,
        });
    }

    let manifest = BackupManifest {
        manifest_version: 1,
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: now,
        source_os: std::env::consts::OS.to_string(),
        source_os_version: os_info::get().version().to_string(),
        wave_link: wave_link_info,
        local_state_relative_path: "payload".to_string(),
        files: entries,
    };

    let live_channels_snapshot = probe.as_ref().and_then(|p| p.channels.clone());
    write_backup_archive(
        &backup_path,
        &manifest,
        &files,
        snapshot.path(),
        live_channels_snapshot,
    )?;

    // Guard against silently returning a corrupted archive.
    // A single short retry is defense-in-depth for transient IO timing.
    let mut inspection = inspect_backup(&backup_path)?;
    if !inspection.valid_hashes {
        thread::sleep(Duration::from_millis(60));
        inspection = inspect_backup(&backup_path)?;
    }
    if !inspection.valid_hashes {
        let warnings = if inspection.warnings.is_empty() {
            "unknown validation error".to_string()
        } else {
            inspection.warnings.join("; ")
        };
        let _ = fs::remove_file(&backup_path);
        return Err(format!(
            "Backup archive failed post-write validation: {warnings}"
        ));
    }

    Ok(BackupCreateResponse {
        backup_path: backup_path.to_string_lossy().to_string(),
        manifest,
    })
}

fn snapshot_local_state(source_root: &Path) -> Result<TempDir, String> {
    let snapshot = tempdir().map_err(|e| e.to_string())?;
    let snapshot_root = snapshot.path();

    for entry in WalkDir::new(source_root).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        let source_path = entry.path();
        let rel = source_path
            .strip_prefix(source_root)
            .map_err(|e| e.to_string())?;
        if rel.as_os_str().is_empty() {
            continue;
        }
        let destination = snapshot_root.join(rel);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
            continue;
        }

        if entry.file_type().is_file() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(source_path, destination).map_err(|e| e.to_string())?;
        }
    }

    Ok(snapshot)
}

fn ensure_backup_extension(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.to_ascii_lowercase().ends_with(".wlbk") {
        trimmed.to_string()
    } else {
        format!("{trimmed}.wlbk")
    }
}

pub fn inspect_backup(path: &Path) -> Result<BackupInspectionResponse, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
    let manifest: BackupManifest = read_manifest(&mut archive)?;

    let mut warnings = Vec::new();
    let mut valid_hashes = true;

    for entry in &manifest.files {
        let expected_path = format!("payload/{}", entry.relative_path.replace('\\', "/"));
        let mut zip_file = match archive.by_name(&expected_path) {
            Ok(file) => file,
            Err(_) => {
                warnings.push(format!("Missing file in archive: {}", entry.relative_path));
                valid_hashes = false;
                continue;
            }
        };
        let mut data = Vec::new();
        zip_file.read_to_end(&mut data).map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let actual_hash = hex::encode(hasher.finalize());
        if actual_hash != entry.sha256 {
            valid_hashes = false;
            warnings.push(format!("Checksum mismatch for {}", entry.relative_path));
        }
    }

    Ok(BackupInspectionResponse {
        backup_path: path.to_string_lossy().to_string(),
        manifest,
        valid_hashes,
        warnings,
    })
}

pub fn extract_backup_to_dir(path: &Path, target_dir: &Path) -> Result<BackupManifest, String> {
    if target_dir.exists() {
        fs::remove_dir_all(target_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(target_dir).map_err(|e| e.to_string())?;

    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
    let manifest = read_manifest(&mut archive)?;

    for entry in &manifest.files {
        let zip_path = format!("payload/{}", entry.relative_path.replace('\\', "/"));
        let mut zf = archive.by_name(&zip_path).map_err(|e| e.to_string())?;
        let out_path = target_dir.join(&entry.relative_path);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out_file = fs::File::create(&out_path).map_err(|e| e.to_string())?;
        std::io::copy(&mut zf, &mut out_file).map_err(|e| e.to_string())?;
    }

    Ok(manifest)
}

pub fn read_live_channels_snapshot(path: &Path) -> Result<Option<serde_json::Value>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut snapshot = match archive.by_name("meta/live-channels.json") {
        Ok(file) => file,
        Err(_) => return Ok(None),
    };
    let mut text = String::new();
    snapshot.read_to_string(&mut text).map_err(|e| e.to_string())?;
    let json = serde_json::from_str::<serde_json::Value>(&text).map_err(|e| e.to_string())?;
    Ok(Some(json))
}

pub fn list_backups() -> Result<Vec<BackupListItem>, String> {
    let backup_root = default_backup_root();
    if !backup_root.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&backup_root).map_err(|e| e.to_string())?;
    let mut backups = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("wlbk"))
            != Some(true)
        {
            continue;
        }

        let metadata = fs::metadata(&path).map_err(|e| e.to_string())?;
        let created_at = metadata
            .modified()
            .map(chrono::DateTime::<Utc>::from)
            .unwrap_or_else(|_| Utc::now());
        let display_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| "Backup".to_string());
        let is_valid = Some(quick_validate_backup(&path));

        backups.push(BackupListItem {
            path: path.to_string_lossy().to_string(),
            display_name,
            created_at,
            size_bytes: metadata.len(),
            is_valid,
        });
    }

    backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(backups)
}

pub fn import_backup_file(source_path: &Path, overwrite: bool) -> Result<ImportBackupResponse, String> {
    if !source_path.exists() || !source_path.is_file() {
        return Err("Selected backup file was not found".to_string());
    }
    if source_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("wlbk"))
        != Some(true)
    {
        return Err("Only .wlbk backup files can be imported".to_string());
    }

    // Validate archive before copying into the managed backup folder.
    let inspection = inspect_backup(source_path)?;
    if !inspection.valid_hashes {
        return Err("Imported backup failed integrity checks".to_string());
    }

    let backup_root = default_backup_root();
    fs::create_dir_all(&backup_root).map_err(|e| e.to_string())?;
    let file_name = source_path
        .file_name()
        .ok_or("Invalid source backup filename")?;
    let target_path = backup_root.join(file_name);
    let same_file = source_path
        .canonicalize()
        .ok()
        .zip(target_path.canonicalize().ok())
        .map(|(a, b)| a == b)
        .unwrap_or(false);
    if same_file {
        return Ok(ImportBackupResponse {
            backup_path: target_path.to_string_lossy().to_string(),
            overwritten: false,
        });
    }

    let existed_before = target_path.exists();
    if existed_before && !overwrite {
        return Err(format!(
            "Backup '{}' already exists",
            file_name.to_string_lossy()
        ));
    }

    fs::copy(source_path, &target_path).map_err(|e| e.to_string())?;
    Ok(ImportBackupResponse {
        backup_path: target_path.to_string_lossy().to_string(),
        overwritten: existed_before && overwrite,
    })
}

pub fn delete_backup_file(path: &Path) -> Result<(), String> {
    let backup_root = default_backup_root()
        .canonicalize()
        .map_err(|e| format!("Backup folder is unavailable: {e}"))?;
    let candidate = path
        .canonicalize()
        .map_err(|_| "Backup file was not found".to_string())?;
    if !candidate.starts_with(&backup_root) {
        return Err("Can only delete backups from the managed backup folder".to_string());
    }
    fs::remove_file(candidate).map_err(|e| e.to_string())
}

fn read_manifest(archive: &mut ZipArchive<fs::File>) -> Result<BackupManifest, String> {
    let mut manifest_file = archive
        .by_name("manifest.json")
        .map_err(|e| e.to_string())?;
    let mut manifest_text = String::new();
    manifest_file
        .read_to_string(&mut manifest_text)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&manifest_text).map_err(|e| e.to_string())
}

fn quick_validate_backup(path: &Path) -> bool {
    let file = match fs::File::open(path) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let mut archive = match ZipArchive::new(file) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let mut manifest_file = match archive.by_name("manifest.json") {
        Ok(v) => v,
        Err(_) => return false,
    };
    let mut manifest_text = String::new();
    if manifest_file.read_to_string(&mut manifest_text).is_err() {
        return false;
    }
    serde_json::from_str::<BackupManifest>(&manifest_text).is_ok()
}

fn write_backup_archive(
    output_path: &Path,
    manifest: &BackupManifest,
    files: &[(String, PathBuf)],
    local_state_root: &Path,
    live_channels_snapshot: Option<serde_json::Value>,
) -> Result<(), String> {
    let out = fs::File::create(output_path).map_err(|e| e.to_string())?;
    let mut zip = ZipWriter::new(out);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("manifest.json", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(
        serde_json::to_string_pretty(manifest)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
    .map_err(|e| e.to_string())?;

    let settings_full = settings_path(local_state_root);
    if settings_full.exists() {
        let settings_text = fs::read_to_string(&settings_full).map_err(|e| e.to_string())?;
        let normalized = serde_json::from_str::<serde_json::Value>(&settings_text)
            .and_then(|json| serde_json::to_string_pretty(&json))
            .unwrap_or(settings_text);

        zip.start_file("settings.normalized.json", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(normalized.as_bytes())
            .map_err(|e| e.to_string())?;
    }

    let ws_info = ws_info_path(local_state_root);
    if ws_info.exists() {
        let ws_text = fs::read_to_string(ws_info).map_err(|e| e.to_string())?;
        zip.start_file("meta/ws-info.json", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(ws_text.as_bytes())
            .map_err(|e| e.to_string())?;
    }

    if let Some(snapshot) = live_channels_snapshot {
        zip.start_file("meta/live-channels.json", options)
            .map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?;
        zip.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    }

    for (relative, absolute) in files {
        let zip_path = format!("payload/{}", relative.replace('\\', "/"));
        zip.start_file(zip_path, options)
            .map_err(|e| e.to_string())?;
        let mut file = fs::File::open(absolute).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut zip).map_err(|e| e.to_string())?;
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn read_ws_port(local_state: &Path) -> Option<u16> {
    let path = ws_info_path(local_state);
    let content = fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let value = json.get("port")?.as_u64()?;
    Some(value as u16)
}

fn collect_files(root: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let mut out = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().is_file() {
            continue;
        }
        let abs = entry.path().to_path_buf();
        let rel = abs
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .to_string();
        out.push((rel, abs));
    }
    Ok(out)
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::tempdir;

    #[test]
    fn snapshot_local_state_copies_all_files() {
        let source = tempdir().expect("create source");
        let nested = source.path().join("Logs/Nested");
        fs::create_dir_all(&nested).expect("create nested");

        let settings = source.path().join("Settings.json");
        let log = source.path().join("Logs/Nested/trace.log");
        fs::write(&settings, br#"{"ok":true}"#).expect("write settings");
        fs::write(&log, b"hello log").expect("write log");

        let snapshot = snapshot_local_state(source.path()).expect("snapshot");
        assert!(snapshot.path().join("Settings.json").exists());
        assert!(snapshot.path().join("Logs/Nested/trace.log").exists());
        assert_eq!(
            fs::read(snapshot.path().join("Settings.json")).expect("read snap settings"),
            fs::read(settings).expect("read source settings")
        );
    }

    #[test]
    fn backup_created_from_snapshot_remains_valid_if_source_changes_after_snapshot() {
        let source = tempdir().expect("create source");
        let log_dir = source.path().join("Logs");
        fs::create_dir_all(&log_dir).expect("create log dir");

        let source_log = log_dir.join("ElgatoWaveLink.log");
        let initial_bytes = b"initial log bytes".to_vec();
        fs::write(&source_log, &initial_bytes).expect("write initial log");

        let snapshot = snapshot_local_state(source.path()).expect("snapshot");

        // Simulate a volatile live file changing immediately after snapshot creation.
        fs::write(&source_log, b"mutated live log bytes").expect("mutate source");

        let mut files = collect_files(snapshot.path()).expect("collect snapshot files");
        files.sort_by(|a, b| a.0.cmp(&b.0));

        let mut entries = Vec::with_capacity(files.len());
        for (rel, abs) in &files {
            entries.push(BackupFileEntry {
                relative_path: rel.clone(),
                size: fs::metadata(abs).expect("metadata").len(),
                sha256: file_sha256(abs).expect("hash"),
            });
        }

        let manifest = BackupManifest {
            manifest_version: 1,
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            created_at: Utc::now(),
            source_os: std::env::consts::OS.to_string(),
            source_os_version: os_info::get().version().to_string(),
            wave_link: None,
            local_state_relative_path: "payload".to_string(),
            files: entries,
        };

        let out_dir = tempdir().expect("create output");
        let archive_path = out_dir.path().join("snapshot.wlbk");
        write_backup_archive(
            &archive_path,
            &manifest,
            &files,
            snapshot.path(),
            None,
        )
        .expect("write archive");

        let inspection = inspect_backup(&archive_path).expect("inspect archive");
        assert!(inspection.valid_hashes);

        let extracted = tempdir().expect("extract dir");
        extract_backup_to_dir(&archive_path, extracted.path()).expect("extract archive");
        assert_eq!(
            fs::read(extracted.path().join("Logs/ElgatoWaveLink.log")).expect("read extracted"),
            initial_bytes
        );
    }

    #[test]
    fn create_backup_produces_valid_archive_single_attempt() {
        let source = tempdir().expect("create source");
        fs::create_dir_all(source.path().join("Logs")).expect("create logs");
        fs::write(source.path().join("Settings.json"), br#"{"mixer":"ok"}"#)
            .expect("write settings");
        fs::write(source.path().join("Logs/runtime.log"), b"runtime")
            .expect("write runtime log");

        let output = tempdir().expect("create output");
        let created = create_backup_from_local_state(
            BackupOptions {
                output_dir: Some(output.path().to_string_lossy().to_string()),
                backup_name: Some("single-attempt".to_string()),
            },
            source.path(),
        )
        .expect("create backup");

        assert!(Path::new(&created.backup_path).exists());
        let inspection = inspect_backup(Path::new(&created.backup_path)).expect("inspect backup");
        assert!(inspection.valid_hashes);
    }
}
