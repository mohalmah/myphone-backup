# UI Overhaul Design Spec

## Problem

The current UI has several issues:
1. Resizable side panel creates a wide gap/strip between panels (egui separator rendering bug)
2. No scrolling — content gets clipped when window is small (progress, file queue hidden)
3. Too much text — labels and descriptions take space that could be icons with hover tooltips
4. Side panel is heavy — Connection, Backup Destination, Validation all fully expanded

## Solution: Compact Responsive Layout

### Layout Architecture

```
+----------------------------------------------------------+
| HEADER: Title | Status Banner | [NO DEVICE] [IDLE]       |
| [Backup] [Cleanup]                                        |
+----------------+-----------------------------------------+
| SIDE PANEL     | CENTRAL PANEL (ScrollArea)               |
| (ScrollArea)   |                                          |
| - Presets      | - Summary strip                          |
| - Device       | - Source Library (compact cards)          |
| - Destination  | - Progress bars                          |
| - Validation   | - File Queue table                        |
|                |                                          |
|----------------|                                          |
| RUN CONTROLS   |                                          |
| (sticky bottom)|                                          |
+----------------+-----------------------------------------+
| ACTIVITY LOG (resizable bottom panel, same as current)    |
+----------------------------------------------------------+
```

**Header:** Same as current `TopBottomPanel::top("hero")`. Title, description, status banner (`self.status_banner`), error banner (`self.error_banner`), device status pills, and Backup/Cleanup tab selector. No changes except minor spacing tightening.

### Side Panel

**Fixed width, responsive:** `(window_width * 0.28).clamp(300.0, 420.0)`
- `resizable(false)` — eliminates the gap bug
- `show_separator_line(false)` — clean edge with a right border stroke
- Content split: Run Controls at bottom (fixed), everything else in `ScrollArea` above

**Run Controls implementation:** Inside the side panel's `show()` closure, render Run Controls first using `ui.with_layout(Layout::bottom_up(Align::Center))` to reserve the bottom strip, then render the `ScrollArea` in the remaining space above.

#### Icons Strategy

All icons use **Unicode text characters** — no new dependencies needed. The current codebase already uses emoji in preset badges (💬, ✈, ⬇, 📷, 📁). We continue this approach:

| Purpose | Character | Fallback |
|---------|-----------|----------|
| Refresh | ↻ | "Refresh" text |
| Browse folder | ... | "..." text |
| Add source | + | "+" text |
| Remove | ✕ | "X" text |
| Pick folder | 📁 | "Pick" text |
| Save | 💾 | "Save" text |
| Start | ▶ | "Start" text |
| Pause | ⏸ | "Pause" text |
| Resume | ▶ | "Resume" text |
| Stop | ⏹ | "Stop" text |
| Retry | ↻ | "Retry" text |

Since the app already bundles Windows system fonts (Tahoma, Arial, Segoe UI) which support these characters, no additional font loading is needed. If a glyph doesn't render, the fallback text is used via a helper: `fn icon_or_text(icon: &str, fallback: &str) -> String`.

#### Presets (always visible at top)
- Chips with app icons (same emoji as current), tighter spacing: `Margin::symmetric(8, 5)`
- Below chips: single 💾 icon button + preset name field inline

#### Device (single compact row)
- Status pill (CONNECTED/NO DEVICE) + model name (if connected) + ↻ refresh icon button
- No card frame, no "ADB executable" field visible (moved to Validation section)
- Hover tooltip on status pill shows: serial, ADB path, full status message

#### Destination (single row)
- Text field with placeholder "Destination folder..." + "..." browse button
- No label text
- Small text below: "N sources enabled"

#### Validation (collapsible, collapsed by default)
- `CollapsingHeader::new("Validation")`
- Validation mode dropdown
- Existing file behavior dropdown
- Auto-delete checkbox
- Dry-run checkbox
- Recent files filter
- ADB path field (advanced, moved here from Connection)

#### Run Controls (sticky bottom, outside ScrollArea)

Three buttons in a horizontal row, always visible:

| State | Start | Pause/Resume | Stop |
|-------|-------|-------------|------|
| Idle | ▶ Start (enabled) | ⏸ (disabled) | ⏹ (disabled) |
| Running | ▶ Start (disabled) | ⏸ Pause (enabled) | ⏹ (enabled) |
| Paused | ▶ Start (disabled) | ▶ Resume (enabled) | ⏹ (enabled) |

These map to the existing `start_full_backup()`, `SyncHandle::toggle_pause()`, and `SyncHandle::request_stop()` methods. The pause functionality already exists in the current code — this is just a visual rearrangement.

### Central Panel

Entire panel wrapped in a single `ScrollArea::vertical()`.

#### Summary Strip (top)
- Metric chips: Total Files, Processed, Speed, ETA
- After run: second row with Copied/Deleted/Skipped/Failed/Conflicts

#### Source Library (compact cards)
- Header row: "Backup Sources" label + "+" Add button + "Scan" button + "Analyze" button
- Destination row: "Dest:" label + path text field + "..." browse button
- Each source card:
  - Checkbox + label text field + 📁 pick folder icon + ✕ remove icon (right-aligned)
  - Source path + " → " + destination subfolder on one compact line (smaller text)
  - Full paths on hover tooltip, truncated display if wider than available width
  - Scan results inline: "42 files | 1.2 GB" or error in red
- Subfolder is an editable text field inline (no separate "Pick Destination" button)

#### Progress
- Two progress bars (overall + current file)
- Detail line (`progress_detail()`) on hover over the progress bar instead of always visible

#### File Queue
- Same table structure with vertical scroll
- Retry button as ↻ icon only

### Activity Log

Same as current: `TopBottomPanel::bottom("log_panel")` with `resizable(true)` and `default_height(230.0)`. No changes — the current implementation works well.

### Remote Folder Picker Modal

No changes. The existing `show_remote_folder_picker()` modal dialog stays as-is.

### Error Banner

Same as current: `self.error_banner` rendered in the header panel below the status banner, in red. No changes.

### Cleanup Tab

Same structural changes apply:
- Cleanup folder field uses placeholder text + "..." browse button
- Delete Actions section stays as-is (safety-critical, needs full text)
- Collapsible sections where appropriate

### Text Reduction Strategy

| Before | After |
|--------|-------|
| "ADB executable" label + field | Hidden in Validation collapsible |
| "Local destination folder" label | Placeholder "Destination folder..." |
| "Select Windows Folder..." button | "..." button |
| "Scan Configured Sources" button | "Scan" button |
| "Analyze Selected Sources" button | "Analyze" button |
| "Add Custom Source" button | "+" button |
| Device model/serial block | Hover tooltip on status pill |
| Explanatory text under fields | Hover tooltips |
| "Save Settings" / "Save as Preset" | Single 💾 icon button |
| "Pick Folder" / "Remove" buttons | 📁 and ✕ icon buttons |
| "Pick Destination" per-card button | Inline editable subfolder text |
| Progress detail text | Hover on progress bar |

### File Decomposition

The current `app.rs` is ~3000 lines. Split into focused modules:

**New file structure:**
```
src/
  app.rs          — BackupApp struct, state, polling, eframe::App impl (update loop only)
  ui/
    mod.rs        — re-exports
    side_panel.rs — side panel rendering
    central_panel.rs — central panel rendering
    widgets.rs    — reusable helpers (chips, cards, icon_or_text, etc.)
    theme.rs      — apply_theme, install_text_fonts, color constants
```

**State passing approach:** Each UI module exposes a single public function that takes `&mut BackupApp` directly. Since `BackupApp` and all its fields are in `app.rs` (same crate), the UI functions access fields directly. No traits or accessor methods needed. Note: `BackupApp` fields are currently private — they need to be changed to `pub(crate)` so the `ui::` submodules can access them.

```rust
// src/ui/side_panel.rs
pub fn render_side_panel(ctx: &egui::Context, app: &mut BackupApp);

// src/ui/central_panel.rs
pub fn render_central_panel(ctx: &egui::Context, app: &mut BackupApp);

// src/ui/widgets.rs
pub fn icon_or_text(icon: &str, fallback: &str) -> String;
pub fn render_preset_chip(ui: &mut egui::Ui, preset: &BackupPreset, selected: bool) -> egui::Response;
pub fn settings_card(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui));
// ... other widget helpers
```

The `update()` method in `app.rs` becomes:
```rust
fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
    self.poll_all();  // all receiver polling
    ui::render_header(ctx, self);
    ui::render_side_panel(ctx, self);
    ui::render_log_panel(ctx, self);
    ui::render_central_panel(ctx, self);
    ui::render_remote_folder_picker(ctx, self);
}
```

**Implementation order:** Refactor file decomposition FIRST (extract without visual changes), then apply visual changes in the new files. Two separate commits.

### Non-Goals
- No new features (no new backup capabilities)
- No data model changes
- No ADB/sync logic changes
- No new crate dependencies
