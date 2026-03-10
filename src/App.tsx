import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useState } from "react";
import {
  createBackupWithName,
  deleteBackup,
  detectInstallation,
  executeRestore,
  importBackup,
  inspectBackup,
  listBackups,
  planRestore,
  terminateWaveLink,
} from "./api";
import type { BackupListItem } from "./types";
import "./App.css";

function App() {
  const [backups, setBackups] = useState<BackupListItem[]>([]);
  const [isBusy, setIsBusy] = useState(false);
  const [busyMessage, setBusyMessage] = useState("");
  const [toast, setToast] = useState<{ id: number; message: string; kind: "success" | "error" } | null>(null);
  const [backupName, setBackupName] = useState("");

  const [selectedBackup, setSelectedBackup] = useState<BackupListItem | null>(null);
  const [restoreModalOpen, setRestoreModalOpen] = useState(false);
  const [launchAfterRestore, setLaunchAfterRestore] = useState(true);
  const [restoreMessage, setRestoreMessage] = useState("");
  const [waveLinkOpen, setWaveLinkOpen] = useState(false);
  const [deleteCandidate, setDeleteCandidate] = useState<BackupListItem | null>(null);
  const [overwriteImportSourcePath, setOverwriteImportSourcePath] = useState<string | null>(null);

  const refreshData = useCallback(async () => {
    const backupItems = await listBackups();
    setBackups(backupItems);
  }, []);

  useEffect(() => {
    void (async () => {
      try {
        await refreshData();
      } catch (err) {
        setAppError("Could not refresh backup data.", normalizeErrorDetails(err));
      }
    })();
  }, [refreshData]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      if (!isBusy) {
        void refreshData().catch(() => undefined);
      }
    }, 2000);
    return () => window.clearInterval(timer);
  }, [isBusy, refreshData]);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), toast.kind === "error" ? 4200 : 2600);
    return () => window.clearTimeout(timer);
  }, [toast, toast?.kind]);

  async function withBusy(action: () => Promise<void>) {
    try {
      setIsBusy(true);
      setBusyMessage("Working...");
      await action();
    } catch (err) {
      setAppError(normalizeError(err), normalizeErrorDetails(err));
    } finally {
      setIsBusy(false);
      setBusyMessage("");
    }
  }

  async function handleCreateBackup() {
    await withBusy(async () => {
      const created = await createBackupWithName(backupName);
      showToast(`Backup created: ${formatBackupLabel(created.backupPath)}`);
      setBackupName("");
      clearAppError();
      await refreshData();
    });
  }

  async function handleImportBackup() {
    const selected = await open({
      directory: false,
      multiple: false,
      title: "Import Wave Link Backup",
      filters: [{ name: "Wave Link Backup", extensions: ["wlbk"] }],
    });
    if (typeof selected !== "string") return;

    await withBusy(async () => {
      try {
        const imported = await importBackup(selected, false);
        showToast(`Imported backup: ${formatBackupLabel(imported.backupPath)}`);
      } catch (err) {
        const message = normalizeError(err);
        if (message.toLowerCase().includes("already exists")) {
          setOverwriteImportSourcePath(selected);
          return;
        } else {
          throw err;
        }
      }
      clearAppError();
      await refreshData();
    });
  }

  function handleDeleteBackup(item: BackupListItem) {
    setDeleteCandidate(item);
  }

  async function confirmDeleteBackup() {
    if (!deleteCandidate) return;
    await withBusy(async () => {
      await deleteBackup(deleteCandidate.path);
      showToast(`Deleted backup: ${formatBackupLabel(deleteCandidate.displayName)}`);
      setDeleteCandidate(null);
      clearAppError();
      await refreshData();
    });
  }

  async function confirmOverwriteImport() {
    if (!overwriteImportSourcePath) return;
    await withBusy(async () => {
      const overwritten = await importBackup(overwriteImportSourcePath, true);
      showToast(`Overwritten backup: ${formatBackupLabel(overwritten.backupPath)}`);
      setOverwriteImportSourcePath(null);
      clearAppError();
      await refreshData();
    });
  }

  async function openRestoreModal(backup: BackupListItem) {
    await withBusy(async () => {
      let processRunning = false;
      try {
        const install = await detectInstallation();
        processRunning = install.processRunning;
      } catch {
        processRunning = false;
      }

        setWaveLinkOpen(processRunning);
      setSelectedBackup(backup);
      setRestoreMessage("");
      setRestoreModalOpen(true);
    });
  }

  function closeRestoreModal() {
    setRestoreModalOpen(false);
    setRestoreMessage("");
    setSelectedBackup(null);
    setWaveLinkOpen(false);
  }

  async function handleRestoreConfirm() {
    if (!selectedBackup) return;

    try {
      setIsBusy(true);
      setBusyMessage(waveLinkOpen ? "Closing Wave Link..." : "Preparing restore...");
      clearAppError();
      setRestoreMessage("");

      if (waveLinkOpen) {
        await terminateWaveLink();
      }

      setBusyMessage("Validating backup...");
      const inspection = await inspectBackup(selectedBackup.path);
      if (!inspection.validHashes) {
        throw new Error("Backup file is invalid or corrupted.");
      }

      setBusyMessage("Planning restore...");
      const restorePlan = await planRestore(selectedBackup.path);

      setBusyMessage("Applying backup and restarting Wave Link...");
      const result = await executeRestore(
        restorePlan.planId,
        restorePlan.summary.unresolvedCount > 0,
        launchAfterRestore,
      );

      setBusyMessage("Finishing up...");
      showToast(result.message);
      closeRestoreModal();
      await refreshData();
    } catch (err) {
      const message = normalizeError(err);
      if (message.toLowerCase().includes("must be closed before restore")) {
        setWaveLinkOpen(true);
        setRestoreMessage(
          "Wave Link is open. Click 'Close Wave Link and Restore' to continue.",
        );
      } else {
        setRestoreMessage(message);
        setAppError("Restore failed.", normalizeErrorDetails(err));
      }
    } finally {
      setIsBusy(false);
      setBusyMessage("");
    }
  }

  function clearAppError() {
    // no-op: errors are surfaced as toasts
  }

  function setAppError(message: string, details: string) {
    console.error(message, details);
    showToast(message, "error");
  }

  function showToast(message: string, kind: "success" | "error" = "success") {
    setToast({ id: Date.now(), message, kind });
  }

  return (
    <div className="workspace">
      <header className="hero">
        <div className="brand-row">
          <span className="status-dot" />
          <div>
            <h1>Wave Link Backup Tool</h1>
          </div>
        </div>
        <p className="subtle">
          Create, import, restore, and delete backups in one place.
        </p>
        <div className="create-row">
          <input
            value={backupName}
            onChange={(e) => setBackupName(e.target.value)}
            placeholder="Optional backup name"
            disabled={isBusy}
          />
          <button className="primary" disabled={isBusy} onClick={handleCreateBackup}>
            Create Backup
          </button>
          <button disabled={isBusy} onClick={handleImportBackup}>
            Import Backup
          </button>
        </div>
      </header>

      <section className="panel">
        <div className="panel-head">
          <h2>Backups</h2>
          <span>{backups.length} total</span>
        </div>

        {backups.length === 0 ? (
          <div className="empty">No backups yet. Create or import one to get started.</div>
        ) : (
          <div className="backup-scroll">
            <ul className="backup-list">
              {backups.map((backup) => (
                <li key={backup.path} className="backup-item">
                  <div className="backup-main">
                    <p className="backup-name">{formatBackupLabel(backup.displayName)}</p>
                    <p className="backup-meta">
                      {new Date(backup.createdAt).toLocaleString()} - {formatSize(backup.sizeBytes)}
                    </p>
                  </div>
                  <div className="backup-actions">
                    <span className={`pill ${backup.isValid === false ? "bad" : "ok"}`}>
                      {backup.isValid === false ? "Invalid" : "Ready"}
                    </span>
                    <button
                      className="primary"
                      disabled={isBusy}
                      onClick={() => openRestoreModal(backup)}
                    >
                      Restore
                    </button>
                    <button disabled={isBusy} onClick={() => handleDeleteBackup(backup)}>
                      Delete
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          </div>
        )}
      </section>

      {restoreModalOpen && selectedBackup ? (
        <div className="modal-backdrop" role="presentation">
          <div className="modal" role="dialog" aria-modal="true" aria-label="Restore backup">
            <h3>Restore Backup</h3>
            <p className="modal-summary">{formatBackupLabel(selectedBackup.displayName)}</p>
            <p className="modal-warning">
              {waveLinkOpen
                ? "Wave Link is open. Confirm to close it, restore this backup, then relaunch."
                : "Wave Link is closed. Restore will proceed immediately."}
            </p>

            <label className="toggle">
              <input
                type="checkbox"
                checked={launchAfterRestore}
                onChange={(e) => setLaunchAfterRestore(e.target.checked)}
                disabled={isBusy}
              />
              Launch Wave Link after restore
            </label>

            {restoreMessage ? <p className="modal-error">{restoreMessage}</p> : null}

            <div className="modal-actions">
              <button onClick={closeRestoreModal} disabled={isBusy}>
                Cancel
              </button>
              <button className="primary" onClick={handleRestoreConfirm} disabled={isBusy}>
                {waveLinkOpen ? "Close Wave Link and Restore" : "Restore Now"}
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {deleteCandidate ? (
        <div className="modal-backdrop" role="presentation">
          <div className="modal" role="dialog" aria-modal="true" aria-label="Delete backup">
            <h3>Delete Backup</h3>
            <p className="modal-summary">{formatBackupLabel(deleteCandidate.displayName)}</p>
            <p className="modal-warning">This action cannot be undone.</p>
            <div className="modal-actions">
              <button onClick={() => setDeleteCandidate(null)} disabled={isBusy}>
                Cancel
              </button>
              <button className="danger" onClick={confirmDeleteBackup} disabled={isBusy}>
                Delete Backup
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {overwriteImportSourcePath ? (
        <div className="modal-backdrop" role="presentation">
          <div className="modal" role="dialog" aria-modal="true" aria-label="Overwrite backup">
            <h3>Backup Already Exists</h3>
            <p className="modal-warning">
              A backup with this filename already exists in your backup folder. Overwrite it?
            </p>
            <div className="modal-actions">
              <button onClick={() => setOverwriteImportSourcePath(null)} disabled={isBusy}>
                Cancel
              </button>
              <button className="primary" onClick={confirmOverwriteImport} disabled={isBusy}>
                Overwrite and Import
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {toast ? (
        <div className="toast-wrap" aria-live="polite">
          <div key={toast.id} className={`toast ${toast.kind}`}>
            {toast.message}
          </div>
        </div>
      ) : null}

      {isBusy ? (
        <div className="busy-overlay" aria-live="polite" aria-busy="true">
          <div className="busy-card">
            <span className="busy-spinner" />
            <span>{busyMessage || "Working..."}</span>
          </div>
        </div>
      ) : null}
    </div>
  );
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  return `${(mb / 1024).toFixed(1)} GB`;
}

function formatBackupLabel(value: string): string {
  return value
    .replace(/\.wlbk$/i, "")
    .replace(/^wavelink-backup-/, "")
    .replace(/^pre-restore-/, "pre-restore ");
}

function normalizeError(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return "Unknown error";
}

function normalizeErrorDetails(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.stack ?? err.message;
  try {
    return JSON.stringify(err, null, 2);
  } catch {
    return "No additional details available.";
  }
}

export default App;
