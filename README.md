# ADB Smart Backup & Cleanup

ADB Smart Backup & Cleanup is a Windows desktop app for safely backing up Android media folders over ADB, validating each transfer, and optionally cleaning up files on the phone only after a successful copy.

It is built with Rust and `egui`, and is designed around a simple rule:

`copy -> validate -> optionally delete`

## What It Does

- Detects an Android device connected through ADB
- Lets you back up one or many Android folders in a single run
- Includes ready-to-use presets for WhatsApp, Telegram, Downloads, and Camera
- Scans configured source folders and shows file counts and total size before backup
- Checks destination free space before a run starts
- Warns when the local system drive may be too low on space for the transfer
- Validates transferred files by size or MD5 before any delete is allowed
- Supports dry-run mode so you can simulate the job before copying or deleting anything
- Includes a separate cleanup tab for previewing and deleting phone folders, contents, or selected items
- Writes detailed logs, including optional ADB command traces and command output

## Current Presets

Built-in source presets include:

- WhatsApp Images
- WhatsApp Videos
- WhatsApp Documents
- WhatsApp Audio
- Telegram Images
- Telegram Video
- Downloads
- Camera

Built-in backup presets include:

- WhatsApp Essentials
- WhatsApp Full Media
- Messaging Media
- Downloads
- Camera Roll

You can edit built-in presets, disable sources you do not want, and add custom phone folders.

## Requirements

- Windows
- An Android phone with USB debugging enabled
- `adb.exe` available on `PATH`, or the full ADB path entered in the app
- A writable local destination folder with enough space for the backup

## Install ADB On Windows

If `adb` is not already installed on your machine, set it up first.

### Option 1. Use Android Platform Tools

1. Download `SDK Platform-Tools for Windows` from the official Android developer site:
   - [Android SDK Platform-Tools](https://developer.android.com/tools/releases/platform-tools)
2. Extract the archive to a folder such as:
   - `C:\platform-tools`
3. Open PowerShell in that folder and confirm ADB works:

```powershell
.\adb.exe version
```

4. You can use the app in either of these ways:
   - add the platform-tools folder to your Windows `PATH`
   - leave it where it is and enter the full `adb.exe` path inside the app

### Option 2. Add ADB To PATH

If you want `adb` to work from any terminal:

1. Open `System Properties`
2. Open `Environment Variables`
3. Edit the `Path` variable for your user account
4. Add the platform-tools folder, for example:
   - `C:\platform-tools`
5. Open a new PowerShell window and verify:

```powershell
adb version
```

### If Windows Does Not Detect The Phone

Try these checks:

- reconnect the USB cable
- use a data-capable USB cable, not a charge-only cable
- try another USB port
- switch the phone USB mode to `File transfer`
- let Windows finish installing drivers
- if needed, install the OEM USB driver for your phone brand

## Phone Setup

You do not need to install any APK or helper app on the phone for this desktop app to work.

What you need on the phone is:

- Developer options enabled
- USB debugging enabled
- the phone unlocked when connecting the first time
- approval of the ADB authorization prompt

### Step-By-Step On The Phone

1. Open `Settings`.
2. Open `About phone`.
3. Tap `Build number` or your device's equivalent entry 7 times.
4. Go back to `Settings`.
5. Open `Developer options`.
6. Enable `USB debugging`.
7. Connect the phone to the PC with USB.
8. If the phone asks for a USB mode, choose `File transfer` if available.
9. Watch for the RSA authorization prompt on the phone.
10. Tap `Allow`.
11. If you trust the PC, you can also enable `Always allow from this computer`.

### Xiaomi / MIUI / HyperOS Notes

Some Xiaomi devices place the required settings in slightly different locations.

Typical path:

1. Open `Settings`
2. Open `About phone`
3. Tap `OS version` repeatedly until developer mode is enabled
4. Go to `Settings > Additional settings > Developer options`
5. Enable `USB debugging`

On some Xiaomi devices, advanced delete or file-management actions may also work better if `USB debugging (Security settings)` is enabled, but this is device-specific and may not be necessary for simple backup reads.

## Verify ADB Before Opening The App

Before using the app for the first time, confirm that ADB sees the phone.

Run:

```powershell
adb devices
```

Expected result:

- the phone serial appears in the list
- the state is `device`

If you see `unauthorized`:

- unlock the phone
- check for the authorization popup
- tap `Allow`
- run `adb devices` again

If you prefer not to add ADB to `PATH`, you can still use the app by pointing the `ADB executable` field to the exact `adb.exe` location.

## Quick Start

1. Install ADB on Windows.
2. Prepare the phone with Developer options and USB debugging.
3. Run `adb devices` once and make sure the phone shows as `device`.
4. From the repository root, start the packaged release:

```powershell
cd dist\releases
.\adb-smart-backup-v0.7.1-windows-x86_64.exe
```

5. Open the dashboard and confirm the status shows `CONNECTED`.
6. If needed, set the `ADB executable` field to your full `adb.exe` path.
7. Leave `Simulation mode` enabled for the first test.
8. Scan sources, review the analysis, and run a test backup before enabling any delete option.

## Recommended First Test

Use this order the first time:

1. Pick a small throwaway source folder on the phone.
2. Choose a local destination with plenty of free space.
3. Keep `Simulation mode` on and `Delete from phone after validated backup` off.
4. Click `Scan folders`.
5. Click `Analyze`.
6. Review file counts, size, and free-space checks.
7. Run the backup.
8. Turn dry-run off and run again.
9. Only enable auto-delete after you are satisfied that copy and validation are working correctly.

## Backup Workflow

The `Backup` tab is for safe media transfer.

### 1. Connection

- Confirm the device is connected and authorized
- If needed, set the `ADB executable` field to a full path such as `C:\platform-tools\adb.exe`

### 2. Backup Destination

- Choose the local destination folder with the Windows folder picker
- This is the root folder where selected source folders will be copied
- Each enabled source can also define its own destination subfolder

### 3. Presets And Source Library

- Choose a preset to quickly load common folder sets
- Enable or disable individual sources
- Edit the Android source path for any preset entry
- Change the destination subfolder for each source
- Add your own custom Android folders
- Use `Pick Folder` to browse folders on the connected phone

### 4. Scan Folders

`Scan folders` checks the configured phone folders and shows:

- Whether each folder exists
- File count
- Total size
- Any scan error for that source

This helps you decide what to include before starting a backup.

### 5. Analyze The Backup Plan

`Analyze` builds a combined preflight summary for the enabled sources and shows:

- Total files found
- Total bytes found
- Files that need copying
- Files already present locally
- Conflicting files
- Destination free space
- Whether the destination appears to have enough room
- A system drive warning when Windows may be tight on space

### 6. Validation And Delete Settings

Validation modes:

- `File size`
- `MD5 hash`

Existing file behavior:

- `Skip if name + size match`
- `Validate before delete`

Safety toggles:

- `Simulation mode`
- `Delete from phone after validated backup`

Important:

- Device-side deletion is blocked unless the file exists locally and validation passes
- Backup deletion is done per file, never as a bulk folder wipe
- If an error occurs, later delete actions are not supposed to continue blindly

## Cleanup Workflow

The `Cleanup` tab is separate from backup because it is destructive by nature.

Use it when you want to inspect a folder first and then delete:

- the entire folder and its contents
- only the contents while keeping the folder
- only selected files and subfolders

### Cleanup Steps

1. Choose a phone folder.
2. Click `Preview contents`.
3. Review the preview summary:
   - root folder
   - file count
   - folder count
   - total file size
4. Review the fetched entries, ordered by size.
5. Select items if you want a selective delete.
6. Arm the confirmation checkbox.
7. Run one of the delete actions.

The cleanup preview is intended to let you inspect what will be removed before any destructive action runs.

## Logs And Settings

The app writes settings and logs relative to the current working directory.

If you run the packaged release from `dist\releases`, you should expect:

- `config\settings.json`
- `logs\YYYY-MM-DD.txt`

If you run the app from the repo root with `cargo run`, those folders are created in the repo root instead.

The detailed activity log can optionally show:

- ADB command line
- Exit code
- `stdout`
- `stderr`

## Rebuild From Source

### Prerequisites

- Rust toolchain for Windows MSVC
- Cargo
- ADB available for runtime testing

### Development Run

```powershell
cargo run
```

### Run Tests

```powershell
cargo test
```

### Build A Release Binary

```powershell
cargo build --release
```

The optimized executable will be created at:

`target\release\adb-smart-backup.exe`

The packaged local release artifact is staged at:

`dist\releases\adb-smart-backup-v0.7.1-windows-x86_64.exe`

## GitHub Releases

Repository:

- [mohalmah/myphone-backup](https://github.com/mohalmah/myphone-backup)

Releases page:

- [GitHub Releases](https://github.com/mohalmah/myphone-backup/releases)

Release notes prepared in this repo:

- `RELEASE_NOTES.md`

Published release:

- [ADB Smart Backup & Cleanup v0.7.1](https://github.com/mohalmah/myphone-backup/releases/tag/v0.7.1)

Release assets:

- `adb-smart-backup-v0.7.1-windows-x86_64.exe`
- `adb-smart-backup-v0.7.1-windows-x86_64.zip`

## Troubleshooting

### Device shows `UNAUTHORIZED`

- Unlock the phone
- Reconnect USB
- Accept the ADB authorization prompt
- Click `Refresh Device`

### `adb` is not found

- Install Android platform-tools
- Enter the full path to `adb.exe` in the app

### Backup says there is not enough space

- Choose a different destination drive
- Free space on the destination drive
- Review the system-drive warning as well, not just the destination drive

### A phone path contains spaces or non-Latin names

- Use the phone folder picker rather than typing the path manually when possible
- Keep detailed logging enabled if you are diagnosing ADB quoting or filename issues

## Safety Note

This project is meant to reduce risk, not remove the need for care.

For the first real run:

- start with dry-run
- keep auto-delete off
- use a small test folder
- inspect the copied files locally
- only then enable delete behavior

## License

This project is licensed under the MIT License.

See `LICENSE` for the full text.
