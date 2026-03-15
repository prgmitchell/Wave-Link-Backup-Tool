import { invoke } from "@tauri-apps/api/core";
import type {
  BackupCreateResponse,
  BackupInspectionResponse,
  BackupListItem,
  BackupLocationResponse,
  DetectInstallationResponse,
  ExecuteRestoreResponse,
  ImportBackupResponse,
  OperationLogEntry,
  ProbeWsResponse,
  RestorePlan,
} from "./types";

export const detectInstallation = () =>
  invoke<DetectInstallationResponse>("detect_wavelink_installation");

export const probeWaveLinkWs = () => invoke<ProbeWsResponse>("probe_wavelink_ws");

export const createBackup = (outputDir?: string) =>
  invoke<BackupCreateResponse>("create_backup_command", {
    options: {
      outputDir: outputDir || null,
      backupName: null,
    },
  });

export const createBackupWithName = (backupName?: string) =>
  invoke<BackupCreateResponse>("create_backup_command", {
    options: {
      outputDir: null,
      backupName: backupName?.trim() ? backupName.trim() : null,
    },
  });

export const listBackups = () => invoke<BackupListItem[]>("list_backups_command");

export const getBackupLocation = () =>
  invoke<BackupLocationResponse>("get_backup_location_command");

export const setBackupLocation = (path: string) =>
  invoke<BackupLocationResponse>("set_backup_location_command", {
    request: { path },
  });

export const resetBackupLocation = () =>
  invoke<BackupLocationResponse>("reset_backup_location_command");

export const openPathInFileManager = (path: string) =>
  invoke<void>("open_path_in_file_manager", { request: { path } });

export const importBackup = (sourcePath: string, overwrite: boolean) =>
  invoke<ImportBackupResponse>("import_backup_command", { sourcePath, overwrite });

export const deleteBackup = (path: string) =>
  invoke<void>("delete_backup_command", { request: { path } });

export const inspectBackup = (path: string) =>
  invoke<BackupInspectionResponse>("inspect_backup_command", { path });

export const planRestore = (path: string) =>
  invoke<RestorePlan>("plan_restore_command", {
    path,
    options: null,
  });

export const executeRestore = (
  planId: string,
  allowUnresolved: boolean,
  launchAfterRestore: boolean,
) =>
  invoke<ExecuteRestoreResponse>("execute_restore_command", {
    planId,
    confirmation: {
      allowUnresolved,
      launchWavelinkAfterRestore: launchAfterRestore,
      mappingOverrides: null,
    },
  });

export const rollbackLastRestore = () =>
  invoke<ExecuteRestoreResponse>("rollback_last_restore_command");

export const terminateWaveLink = () =>
  invoke<string[]>("terminate_wavelink_processes_command");

export const listLogs = () => invoke<OperationLogEntry[]>("list_operation_logs");
