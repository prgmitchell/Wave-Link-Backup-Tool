# Wave Link Backup Tool

A desktop backup/restore tool for Elgato Wave Link (Tauri + React).

## Important Warning

This project is **early alpha** software.

- Use it at your own risk.
- You are responsible for verifying backups/restores in your own environment.
- The author is **not responsible for any data loss, corruption, or downtime**.

## Platform Status

- Windows: tested and currently the primary target.
- macOS: **untested** (builds exist, but behavior has not been fully validated end-to-end).

## How To Use

1. Open the app.
2. Click `Create Backup` (optional custom name).
3. Restore from the backup list with `Restore`.
4. Use `Import Backup` to bring an external `.wlbk` into the app list.
5. Use `Delete` to remove a backup.

## Build / Run

### Development

```bash
npm install
npm run tauri dev
```

### Production build

```bash
npm run build
npm run tauri build
```

## Backup Storage Location

- Windows: `%LOCALAPPDATA%\Wave Link Backup Tool\Backups`
- macOS: `~/.wavelink-backup-tool/backups`

## GitHub Releases (Tag-Based)

This repo includes a GitHub Actions workflow that builds Windows and macOS artifacts and creates a release when you push a version tag.

Example:

```bash
git tag v0.2.0
git push origin v0.2.0
```
