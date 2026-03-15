export type MappingStatus = "matched" | "autoMapped" | "userMapped" | "unresolved";

export interface DetectInstallationResponse {
  localStatePath?: string;
  wsInfoPath?: string;
  wsPort?: number;
  settingsPath?: string;
  backupDir?: string;
  processRunning: boolean;
  processNames: string[];
  platform: string;
}

export interface ProbeWsResponse {
  connected: boolean;
  endpoint?: string;
  appInfo?: unknown;
  mixes?: unknown;
  channels?: unknown;
  errors: string[];
}

export interface BackupFileEntry {
  relativePath: string;
  size: number;
  sha256: string;
}

export interface BackupManifest {
  manifestVersion: number;
  toolVersion: string;
  createdAt: string;
  sourceOs: string;
  sourceOsVersion: string;
  localStateRelativePath: string;
  files: BackupFileEntry[];
}

export interface BackupCreateResponse {
  backupPath: string;
  manifest: BackupManifest;
}

export interface BackupLocationResponse {
  currentPath: string;
  defaultPath: string;
  isCustom: boolean;
}

export interface BackupListItem {
  path: string;
  displayName: string;
  createdAt: string;
  sizeBytes: number;
  isValid?: boolean;
}

export interface ImportBackupResponse {
  backupPath: string;
  overwritten: boolean;
}

export interface BackupInspectionResponse {
  backupPath: string;
  manifest: BackupManifest;
  validHashes: boolean;
  warnings: string[];
}

export interface DeviceMappingDecision {
  sourceId: string;
  sourceName?: string;
  targetId?: string;
  targetName?: string;
  status: MappingStatus;
  reason: string;
}

export interface RestorePlanSummary {
  totalDeviceRefs: number;
  unresolvedCount: number;
  canExecuteWithoutForce: boolean;
}

export interface RestorePlan {
  planId: string;
  backupPath: string;
  generatedAt: string;
  summary: RestorePlanSummary;
  mapping: DeviceMappingDecision[];
  warnings: string[];
}

export interface ExecuteRestoreResponse {
  success: boolean;
  message: string;
  rollbackBackupPath?: string;
  unresolvedCount: number;
}

export interface OperationLogEntry {
  id: string;
  timestamp: string;
  operation: string;
  level: string;
  message: string;
  metadata?: unknown;
}
