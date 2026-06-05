# Release Notes

## v0.7.1

Patch release for UI polish and clearer long-running ADB feedback.

### Included

- Replaced the bad Settings placeholder with a real Settings page for ADB, safety defaults, folders, and logs
- Fixed the left navigation so selected items use a fixed compact size and never stretch into a giant blue block
- Added a global ADB work status strip that appears during device checks, folder browsing, source scans, backup analysis, backup runs, cleanup previews, and delete operations
- Added visible spinner, elapsed time, latest activity, backup progress, and pause/stop or cancel actions where possible
- Added a Windows file picker for selecting `adb.exe`

### Release Assets

- `adb-smart-backup-v0.7.1-windows-x86_64.exe`
- `adb-smart-backup-v0.7.1-windows-x86_64.zip`

## v0.7.0

UI overhaul focused on making the app more abstract, calmer, and easier to operate.

### Included

- Rebuilt the dashboard as a simple launch pad for backup and cleanup
- Simplified the left navigation into compact text-first tabs
- Reworked Backup into a plan-based flow: source packs, destination, selected phone folders, safety, and run
- Moved advanced backup options behind collapsible sections
- Reworked Cleanup into a guarded review flow with folder preview, delete plan, and contents list
- Replaced fragile emoji/mojibake UI badges with plain text app chips such as `WA`, `TG`, `DL`, and `CAM`
- Kept Windows folder selection for destination roots and per-source PC destinations
- Improved the detailed activity log panel with an explicit `Show ADB details` toggle
- Rebuilt the Windows release executable

### Release Assets

- `adb-smart-backup-v0.7.0-windows-x86_64.exe`
- `adb-smart-backup-v0.7.0-windows-x86_64.zip`

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
