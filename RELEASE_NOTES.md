# Release Notes

## v0.6.0

UI revamp release focused on a cleaner, simpler backup workflow.

### Included

- Reworked Backup page into a simpler setup-and-run layout
- Moved presets into a clearer quick-selection area
- Kept multi-preset chip loading and source merging
- Added a Windows folder picker for each source's PC destination in the source library
- Improved per-source destination preview so users can see where each phone folder will copy locally
- Kept preflight analysis, space checks, dry-run mode, validation settings, and run controls in one focused panel
- Kept the recent file queue and retry actions available below the main workflow

### Release Assets

- `adb-smart-backup-v0.6.0-windows-x86_64.exe`
- `adb-smart-backup-v0.6.0-windows-x86_64.zip`

## v0.1.0

First packaged release of ADB Smart Backup & Cleanup.

### Included

- Windows desktop app built with Rust and `egui`
- Safe Android backup flow with per-file validation
- Optional delete-after-success logic
- Dry-run mode
- Destination free-space checks
- System-drive free-space warning
- Backup presets for WhatsApp, Telegram, Downloads, and Camera
- Editable multi-folder source library
- Source scanning with per-folder file count and total size
- Separate cleanup tab with preview-first deletion flow
- Selective cleanup for chosen files and subfolders
- Detailed activity log with optional ADB command tracing

### Release Assets

- `adb-smart-backup-v0.1.0-windows-x86_64.exe`
- `adb-smart-backup-v0.1.0-windows-x86_64.zip`

### GitHub Release Target

- [https://github.com/mohalmah/myphone-backup/releases](https://github.com/mohalmah/myphone-backup/releases)
