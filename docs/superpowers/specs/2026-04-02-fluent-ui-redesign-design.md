# Fluent Design UI Redesign — Design Spec

**Date:** 2026-04-02  
**Version target:** v0.5.0  
**Status:** Approved

---

## Overview

Full UI redesign of ADB Smart Backup & Cleanup to match a Windows 11 Fluent Design reference mockup. The app currently uses a two-panel layout (side panel + central panel). The redesign replaces this with a proper Windows 11 navigation rail (always-visible, icons + labels) and dedicated per-page layouts that mirror the reference image closely.

---

## Goals

- Match the reference mockup's visual structure and Windows 11 aesthetic
- Introduce a Dashboard page as the default landing screen
- Refactor Backup and Cleanup into purpose-built page layouts
- Add Devices and Settings as "Coming Soon" stubs (nav items present, content placeholder)
- Keep all existing functionality; no behaviour regressions

---

## Architecture

### New file structure

```
src/ui/
  mod.rs               — re-exports all UI modules
  theme.rs             — unchanged (Fluent color tokens)
  widgets.rs           — unchanged (shared helpers)
  nav_rail.rs          — NEW: left navigation rail
  dashboard_page.rs    — NEW: Dashboard page
  backup_page.rs       — REPLACES central_panel.rs (backup tab content)
  cleanup_page.rs      — REPLACES central_panel.rs (cleanup tab content)
  coming_soon.rs       — NEW: shared "Coming Soon" placeholder
```

`side_panel.rs` and `central_panel.rs` are removed. All panel-dispatching logic moves into `app.rs` `update()` which calls `nav_rail::render` then the active page renderer.

### AppTab enum

```rust
pub enum AppTab {
    Dashboard,   // default
    Backup,
    Cleanup,
    Devices,     // coming soon
    Settings,    // coming soon
}
```

Default changes from `Backup` to `Dashboard`.

---

## Page Designs

### Shell layout

The window is divided into two vertical regions:

1. **Nav rail** — fixed 88 px wide, always visible, no collapse toggle
2. **Content area** — fills remaining width

The `egui::SidePanel::left("nav_rail")` replaces the existing `settings_v2` side panel. The content area uses `egui::CentralPanel`.

### Nav rail (`nav_rail.rs`)

- 88 px wide, `BG_LAYER` fill, subtle right border
- Hamburger icon at top (decorative, no collapse — always expanded)
- Nav items stacked vertically: icon (16 px) + label (9–10 px) below
- Active item: `BG_CARD` fill + `ACCENT` left indicator bar (3 px)
- Hover: `BG_CARD_HOVER`
- Items: Dashboard (⊞), Backup (📦), Cleanup (🧹), Devices (📱 — dimmed), Settings (⚙ — pinned to bottom)
- Settings pinned to bottom of rail with `ui.with_layout(Layout::bottom_up(...))`

### Dashboard page (`dashboard_page.rs`)

Three sections stacked vertically, all in cards:

1. **Device card** — green/red status pill + device name + serial, "Re-scan" button right-aligned. Below: "LAST BACKUP: [datetime]"
2. **Storage row** — two equal cards side by side: PC Storage and Phone Storage. Each has a label, `ProgressBar` (ACCENT fill), and free/total text.
3. **Actions row** — two equal buttons: "↺ Start New Backup" (accent filled, navigates to Backup tab) and "🧹 Cleanup Phone" (outline, navigates to Cleanup tab)
4. **Log card** — collapsible (default open), shows recent log entries (last 10), scrollable. Uses existing `app.log_entries`.

### Backup page (`backup_page.rs`)

Page title "Backup" at top (20 px bold). Below: a horizontal three-column card layout.

**Column 1 — Step 1: Select Source Folders**
- Label "Step 1: Select Source Folders"
- Above the source list: Presets row — horizontal wrapped chips using existing `render_preset_chip` + "Save preset" input, same logic as current side panel
- Scrollable list of backup sources as checkbox rows. Each row: checkbox, 📁 icon, label. Selected = ticked.
- "＋ Add Custom Phone Folder" link at bottom
- Sources are `app.settings.backup_sources`

**Column 2 — Step 2: Choose PC Destination**
- Label "Step 2: Choose PC Destination"
- Text input + Browse button for `app.settings.destination_path`
- Shows resolved path below input

**Column 3 — Step 3: Analyze & Configure** (two sub-cards)
- *Preflight Check card*: total files, total size, free space on destination (color-coded), conflict count
- *Backup Options card*: Validation mode combo, Existing files behavior combo, Dry-run toggle, Date filter combo
- Action buttons at bottom: "Start Backup" (accent) + "Run Dry-run" (outline)

Below all three columns: full-width toggle bar — "AUTO-DELETE AFTER SUCCESSFUL BACKUP" with a toggle switch using `app.settings.auto_delete_after_success`.

Progress bar (during run) replaces the toggle bar area and shows file progress with pause/stop controls.

The existing `render_remote_folder_picker` window dialog is unchanged.

### Cleanup page (`cleanup_page.rs`)

Page title "Cleanup" at top. Below: breadcrumb navigation bar (back/forward arrows + current phone path + folder/refresh icons).

Two-column layout:

**Left — file browser**
- Sort bar: "Sort by [Largest first ▾]"
- Columnar list: checkbox, Name, Size, "Select" link
- Rows are `app.folder_cleanup.preview.entries`
- Clicking a folder navigates into it (uses existing `request_remote_directory_listing`)
- Checkbox state tied to `app.folder_cleanup.selected_paths`

**Right panel (fixed ~200 px)**
- *Results card*: current file name, files count, speed, ETA, status rows (Copying ✓, Validated ✓, Deleted ✗)
- Pause / Resume buttons
- *Cleanup Options card*: radio buttons — Delete Entire Folder / Delete Contents Only / Delete Selected Items
- "DELETE SELECTED" button (red accent, enabled only when armed + selection exists)

The existing "ARE YOU SURE?" confirmation dialog is preserved as an `egui::Window` modal.

### Devices page (stub)

Centered content: large icon, "Coming Soon" heading, description text, "Go to Dashboard" button that sets `app.active_tab = AppTab::Dashboard`.

### Settings page (stub)

Same as Devices stub.

---

## Theme changes

No changes to `theme.rs` color tokens. The nav rail uses existing `BG_LAYER`, `BG_CARD`, `BORDER_CARD`, `ACCENT` tokens. A new constant `ACCENT_INDICATOR` (3 px left bar on active nav item) reuses `ACCENT`.

---

## Data model changes

- `AppTab` enum: add `Dashboard`, `Devices`, `Settings` variants; default changes to `Dashboard`
- No changes to `Settings`, `BackupApp`, or core models
- `app.rs` `update()` dispatches to new page renderers instead of `render_header` + `render_side_panel` + `render_central_panel`

---

## Removed

- `src/ui/side_panel.rs` — deleted; all controls absorbed into page layouts
- `src/ui/central_panel.rs` — deleted; split into `backup_page.rs` and `cleanup_page.rs`
- `render_header()` and `render_log_panel()` from `central_panel.rs` — log moves into Dashboard; header replaced by page titles within each page

---

## Release

After implementation, bump version to `0.5.0` in `Cargo.toml`, tag `v0.5.0`, create GitHub release with release notes describing the Windows 11 UI redesign.

---

## Known intentional regressions

- **Log panel no longer always visible**: the current bottom log panel is visible on all tabs. After the redesign the log is only accessible on the Dashboard page. This is intentional to match the reference mockup. Users who want to monitor a running backup should stay on the Dashboard or Backup page.

---

## Out of scope

- Devices page actual functionality (future version)
- Settings page actual functionality (future version)
- Dark mode
- Window size persistence changes
