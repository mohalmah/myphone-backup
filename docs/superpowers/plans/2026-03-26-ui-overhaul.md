# UI Overhaul Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix panel gap bug, add scrollability, make UI compact with icons and hover tooltips, decompose app.rs into focused modules.

**Architecture:** Extract UI rendering from app.rs (~3081 lines) into `src/ui/` submodules (theme, widgets, side_panel, central_panel). Convert side panel from resizable to fixed-width responsive. Replace text labels with icons and hover tooltips. Wrap both panels in ScrollAreas.

**Tech Stack:** Rust, egui 0.31 / eframe 0.31, no new dependencies

**Spec:** `docs/superpowers/specs/2026-03-26-ui-overhaul-design.md`

---

## File Structure

```
src/
  app.rs              — MODIFY: keep BackupApp struct + state + polling + eframe::App impl (slim coordinator)
  main.rs             — MODIFY: add `mod ui;` declaration
  ui/
    mod.rs            — CREATE: module declarations and re-exports
    theme.rs          — CREATE: apply_theme, install_text_fonts, color constants
    widgets.rs        — CREATE: icon_or_text, preset chips, metric chips, settings_card, status_pill, etc.
    side_panel.rs     — CREATE: render_side_panel() with compact layout
    central_panel.rs  — CREATE: render_central_panel() with ScrollArea + compact cards
  core/               — NO CHANGES to any core files
```

---

### Task 1: Create ui module skeleton and extract theme

**Files:**
- Create: `src/ui/mod.rs`
- Create: `src/ui/theme.rs`
- Modify: `src/main.rs` (add `mod ui;`)
- Modify: `src/app.rs` (remove theme functions, make fields `pub(crate)`)

- [ ] **Step 1: Create `src/ui/mod.rs`**

```rust
pub mod theme;
```

- [ ] **Step 2: Create `src/ui/theme.rs`**

Move these functions from `app.rs` into `src/ui/theme.rs`:
- `apply_theme()` (app.rs:2333-2357)
- `install_text_fonts()` (app.rs:2359-2386)

Add necessary imports at top:
```rust
use eframe::egui::{self, Color32, Context, FontData, FontDefinitions, FontFamily, Stroke};
```

Make both functions `pub(crate)`.

- [ ] **Step 3: Add `mod ui;` to `src/main.rs`**

In `main.rs`, add `mod ui;` alongside the existing `mod app;` and `mod core;`. This makes the `ui` module available crate-wide as `crate::ui::`.

```rust
mod app;
mod core;
mod ui;
```

- [ ] **Step 4: Update `src/app.rs`**

In `BackupApp::new()`, change `apply_theme(&cc.egui_ctx)` to `crate::ui::theme::apply_theme(&cc.egui_ctx)`. Delete the `apply_theme` and `install_text_fonts` functions from app.rs.

- [ ] **Step 5: Make BackupApp fields `pub(crate)`**

Change all fields in `BackupApp` (app.rs:80-103) from private to `pub(crate)`. Also make the helper structs `pub(crate)`: `RemoteFolderPicker`, `FolderCleanupState`, `BackupAnalysisState`, `BackupSourceLibraryState`, `RemoteFolderPickerTarget`, `AppTab`. Their fields also need `pub(crate)`.

- [ ] **Step 6: Build and verify**

Run: `cargo build 2>&1`
Expected: compiles with no errors

- [ ] **Step 7: Commit**

```bash
git add src/ui/ src/app.rs src/main.rs
git commit -m "refactor: extract theme to src/ui/theme.rs, create ui module skeleton"
```

---

### Task 2: Extract widget helpers to widgets.rs

**Files:**
- Create: `src/ui/widgets.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Create `src/ui/widgets.rs`**

Move these free functions from `app.rs` into `src/ui/widgets.rs`, making them all `pub(crate)`:

- `icon_or_text()` — NEW helper function (see below)
- `contains_arabic()` (app.rs:2388)
- `display_text_for_ui()` (app.rs:2401)
- `wrapped_text()` (app.rs:2420)
- `wrapped_path_text()` (app.rs:2424)
- `settings_card()` (app.rs:2433-2445)
- `PresetBadge` struct (app.rs:2447-2450)
- `render_preset_chip()` (app.rs:2453)
- `preset_badges()` (app.rs:2530)
- `preset_chip_hover_text()` (app.rs:2581)
- `status_pill()` (app.rs:2605)
- `device_summary()` (app.rs:2616)
- `summary_strip()` (app.rs:2628)
- `metric_chip()` (app.rs:2699)
- `progress_detail()` (app.rs:2713)
- `format_bytes()` (app.rs:2727)
- `format_duration()` (app.rs:2748)
- `cleanup_summary()` (app.rs:2763)
- `render_backup_analysis()` (app.rs:2773)
- `render_detailed_log_entry()` (app.rs:2952)
- `log_level_color()` (app.rs:2983)
- `protected_cleanup_folder_reason()` (app.rs:2991)
- `initial_local_directory()` (app.rs:3009)
- `derive_destination_subfolder()` (app.rs:3024)
- `normalize_remote_path()` (app.rs:3055)
- `parent_remote_path()` (app.rs:3069)

Add the new `icon_or_text` helper:
```rust
pub(crate) fn icon_or_text(icon: &str, _fallback: &str) -> String {
    // Unicode icons render fine with bundled Windows fonts (Tahoma, Arial, Segoe UI)
    // This helper exists for future fallback if needed
    icon.to_string()
}
```

- [ ] **Step 2: Update `src/ui/mod.rs`**

```rust
pub mod theme;
pub mod widgets;
```

- [ ] **Step 3: Update `src/app.rs`**

Delete all moved functions from app.rs. Add `use crate::ui::widgets::*;` at the top of app.rs for convenience (all widget helpers used inline).

- [ ] **Step 4: Build and verify**

Run: `cargo build 2>&1`
Expected: compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add src/ui/widgets.rs src/ui/mod.rs src/app.rs
git commit -m "refactor: extract widget helpers to src/ui/widgets.rs"
```

---

### Task 3: Extract side panel rendering

**Files:**
- Create: `src/ui/side_panel.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Create `src/ui/side_panel.rs`**

Create a single public function:
```rust
use crate::ui::widgets::*;

pub(crate) fn render_side_panel(ctx: &egui::Context, app: &mut crate::app::BackupApp) {
    // ... side panel rendering code extracted from app.rs update()
}
```

Extract the `egui::SidePanel::left("settings_panel")...show(ctx, |ui| { ... })` block from app.rs (currently lines ~1332-1761). This includes:
- Quick Presets section
- Connection card
- Backup Destination card
- Cleanup Folder card
- Delete Actions card
- Validation card
- Run Controls card

Keep the logic exactly as-is for now — no visual changes in this task.

- [ ] **Step 2: Update `src/ui/mod.rs`**

```rust
pub mod theme;
pub mod widgets;
pub mod side_panel;
```

- [ ] **Step 3: Update `src/app.rs` update()**

Replace the `egui::SidePanel::left(...)` block in `update()` with:
```rust
crate::ui::side_panel::render_side_panel(ctx, self);
```

- [ ] **Step 4: Build and verify**

Run: `cargo build 2>&1`
Expected: compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add src/ui/side_panel.rs src/ui/mod.rs src/app.rs
git commit -m "refactor: extract side panel rendering to src/ui/side_panel.rs"
```

---

### Task 4: Extract central panel, header, log panel rendering

**Files:**
- Create: `src/ui/central_panel.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Create `src/ui/central_panel.rs`**

Extract into four public functions:

```rust
use crate::ui::widgets::*;

pub(crate) fn render_header(ctx: &egui::Context, app: &mut crate::app::BackupApp) { ... }
pub(crate) fn render_log_panel(ctx: &egui::Context, app: &mut crate::app::BackupApp) { ... }
pub(crate) fn render_central_panel(ctx: &egui::Context, app: &mut crate::app::BackupApp) { ... }
pub(crate) fn render_remote_folder_picker(ctx: &egui::Context, app: &mut crate::app::BackupApp) { ... }
```

Extract from app.rs:
- `egui::TopBottomPanel::top("hero")` block (~lines 1270-1330) → `render_header()`
- `egui::TopBottomPanel::bottom("log_panel")` block (~lines 1763-1808) → `render_log_panel()`
- `egui::CentralPanel::default().show(...)` block (~lines 1810-2300) → `render_central_panel()`
- `show_remote_folder_picker()` method (lines 1111-1242) → `render_remote_folder_picker()`

Keep logic exactly as-is — no visual changes.

- [ ] **Step 2: Update `src/ui/mod.rs`**

```rust
pub mod theme;
pub mod widgets;
pub mod side_panel;
pub mod central_panel;
```

- [ ] **Step 3: Update `src/app.rs` update()**

The `update()` method becomes:
```rust
fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
    self.poll_background_logs();
    self.poll_device_probe();
    self.poll_sync_events();
    self.poll_remote_folder_picker();
    self.poll_cleanup_jobs();
    self.poll_backup_analysis();
    self.poll_backup_source_scan();

    if self.sync_receiver.is_some()
        || self.device_probe_receiver.is_some()
        || self.remote_folder_picker.receiver.is_some()
        || self.folder_cleanup.preview_receiver.is_some()
        || self.folder_cleanup.delete_receiver.is_some()
        || self.backup_analysis.receiver.is_some()
        || self.backup_source_library.scan_receiver.is_some()
    {
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    crate::ui::central_panel::render_header(ctx, self);
    crate::ui::side_panel::render_side_panel(ctx, self);
    crate::ui::central_panel::render_log_panel(ctx, self);
    crate::ui::central_panel::render_central_panel(ctx, self);
    crate::ui::central_panel::render_remote_folder_picker(ctx, self);
}
```

- [ ] **Step 4: Build and verify**

Run: `cargo build 2>&1`
Expected: compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add src/ui/central_panel.rs src/ui/mod.rs src/app.rs
git commit -m "refactor: extract central panel, header, log panel to src/ui/central_panel.rs"
```

---

### Task 5: Fix side panel — fixed width, no gap, scrollable

**Files:**
- Modify: `src/ui/side_panel.rs`

This is where visual changes begin.

- [ ] **Step 1: Change side panel to fixed-width responsive**

In `render_side_panel()`, replace the panel construction:

```rust
let window_width = ctx.screen_rect().width();
let panel_width = (window_width * 0.28).clamp(300.0, 420.0);

egui::SidePanel::left("settings_panel")
    .resizable(false)
    .exact_width(panel_width)
    .show_separator_line(false)
    .frame(Frame::new()
        .fill(Color32::from_rgb(247, 241, 230))
        .inner_margin(Margin::same(10))
        .stroke(Stroke::new(1.0, Color32::from_rgb(221, 211, 190))))
    .show(ctx, |ui| {
        // ... content
    });
```

- [ ] **Step 2: Add sticky Run Controls at bottom, scrollable content above**

Inside the side panel `show` closure, use `bottom_up` layout to reserve the bottom strip for Run Controls, then render the scrollable content above:

```rust
.show(ctx, |ui| {
    let panel_rect = ui.available_rect_before_wrap();

    // Reserve bottom strip for Run Controls
    let controls_height = 50.0;
    let controls_rect = egui::Rect::from_min_max(
        egui::pos2(panel_rect.min.x, panel_rect.max.y - controls_height),
        panel_rect.max,
    );
    let scroll_rect = egui::Rect::from_min_max(
        panel_rect.min,
        egui::pos2(panel_rect.max.x, controls_rect.min.y),
    );

    // Run Controls at bottom (always visible)
    ui.allocate_ui_at_rect(controls_rect, |ui| {
        ui.separator();
        ui.add_space(4.0);
        render_run_controls(ui, app);
    });

    // Scrollable content above
    ui.allocate_ui_at_rect(scroll_rect, |ui| {
        ScrollArea::vertical()
            .id_salt("side_panel_scroll")
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                // all side panel sections (presets, device, destination, validation, cleanup)
            });
    });
});
```

Note: if Run Controls content exceeds 50px at runtime, increase `controls_height` to accommodate. Test with different window sizes to verify.

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add src/ui/side_panel.rs
git commit -m "fix: fixed-width responsive side panel, eliminates gap bug, sticky run controls"
```

---

### Task 6: Compact side panel — device, destination, validation

**Files:**
- Modify: `src/ui/side_panel.rs`

- [ ] **Step 1: Compact Device section**

Replace the "Connection" settings_card with a single horizontal row:

```rust
// Device row (no card frame)
ui.horizontal(|ui| {
    status_pill(ui, app.device_info.state.label(), app.device_info.state.color());
    if let Some(model) = &app.device_info.model {
        ui.label(RichText::new(model).small());
    }
    if ui.button(icon_or_text("↻", "Refresh")).clicked() {
        app.refresh_device_info();
    }
})
.response
.on_hover_text(format!(
    "Serial: {}\nADB: {}\n{}",
    app.device_info.serial,
    app.settings.adb_path,
    app.device_info.message
));
```

- [ ] **Step 2: Compact Destination section**

Replace "Backup Destination" card with a single row:

```rust
ui.add_space(6.0);
ui.horizontal(|ui| {
    let response = ui.add(
        egui::TextEdit::singleline(&mut app.settings.destination_path)
            .desired_width(ui.available_width() - 40.0)
            .hint_text("Destination folder..."),
    );
    if response.changed() {
        app.invalidate_backup_analysis();
    }
    if ui.button("...").on_hover_text("Browse for destination folder").clicked() {
        app.pick_local_destination_folder();
    }
});
let enabled_count = app.settings.effective_backup_sources().iter().filter(|s| s.enabled).count();
ui.small(format!("{enabled_count} sources enabled"));
```

- [ ] **Step 3: Collapsible Validation section**

Replace the Validation settings_card with a CollapsingHeader, collapsed by default. Move ADB path field here:

```rust
egui::CollapsingHeader::new("Validation")
    .default_open(false)
    .show(ui, |ui| {
        ui.label("ADB path");
        ui.add(
            egui::TextEdit::singleline(&mut app.settings.adb_path)
                .desired_width(f32::INFINITY)
                .hint_text("adb"),
        );
        ui.add_space(4.0);
        // ... existing validation dropdowns and checkboxes (same as current)
    });
```

- [ ] **Step 4: Compact Cleanup tab sections**

When `app.active_tab == AppTab::Cleanup`:
- Replace Cleanup Folder settings_card: use placeholder text "Phone folder to clean up..." in the text field, replace "Select Phone Folder..." button with "..." button with hover tooltip
- Keep Delete Actions section as-is (safety-critical, needs full text)

```rust
// Cleanup folder (compact)
ui.add_space(6.0);
ui.horizontal(|ui| {
    let mut cleanup_path = app.folder_cleanup.folder_path.clone();
    if ui.add(
        egui::TextEdit::singleline(&mut cleanup_path)
            .desired_width(ui.available_width() - 40.0)
            .hint_text("Phone folder to clean up..."),
    ).changed() {
        app.set_cleanup_folder_path(cleanup_path);
    }
    if ui.add_enabled(!adb_job_active, egui::Button::new("..."))
        .on_hover_text("Select phone folder")
        .clicked()
    {
        app.open_cleanup_folder_picker();
    }
});
// Fetch/Clear buttons compact
ui.horizontal(|ui| {
    if ui.add_enabled(!adb_job_active, egui::Button::new("Fetch"))
        .on_hover_text("Fetch folder contents from device")
        .clicked()
    {
        app.request_cleanup_preview();
    }
    if ui.add_enabled(!app.folder_cleanup.is_deleting, egui::Button::new("Clear"))
        .on_hover_text("Clear preview")
        .clicked()
    {
        app.clear_cleanup_preview();
    }
});
// ... rest of cleanup (protected folder warnings, spinner, preview, errors) same as current
// Delete Actions card stays as-is (safety-critical)
```

- [ ] **Step 5: Compact Run Controls**

Replace the existing Run Controls with icon buttons:

```rust
fn render_run_controls(ui: &mut egui::Ui, app: &mut crate::app::BackupApp) {
    ui.horizontal(|ui| {
        let running = app.is_running();
        let paused = app.sync_handle.as_ref().map(|h| h.is_paused()).unwrap_or(false);

        if ui.add_enabled(!app.has_active_adb_job(), egui::Button::new("▶ Start")).clicked() {
            app.start_full_backup();
        }
        if ui.add_enabled(running, egui::Button::new(if paused { "▶ Resume" } else { "⏸ Pause" })).clicked() {
            if let Some(handle) = &app.sync_handle {
                handle.toggle_pause();
            }
        }
        if ui.add_enabled(running, egui::Button::new("⏹ Stop")).clicked() {
            if let Some(handle) = &app.sync_handle {
                handle.request_stop();
            }
        }
    });
}
```

- [ ] **Step 6: Build and verify**

Run: `cargo build 2>&1`
Expected: compiles with no errors

- [ ] **Step 7: Commit**

```bash
git add src/ui/side_panel.rs
git commit -m "feat: compact side panel — device row, destination row, collapsible validation, compact cleanup"
```

---

### Task 7: Compact central panel — scrollable, compact source cards

**Files:**
- Modify: `src/ui/central_panel.rs`

- [ ] **Step 1: Wrap entire central panel in ScrollArea**

```rust
egui::CentralPanel::default().show(ctx, |ui| {
    ScrollArea::vertical()
        .id_salt("central_panel_scroll")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            // all central panel content here
        });
});
```

- [ ] **Step 2: Compact source library header with destination row**

Replace the header row with icon buttons, and add destination row inside source library:

```rust
ui.horizontal_wrapped(|ui| {
    ui.label(RichText::new("Backup Sources").strong());
    if ui.add_enabled(!app.has_active_adb_job(), egui::Button::new("+"))
        .on_hover_text("Add custom source")
        .clicked()
    {
        app.add_custom_backup_source();
        backup_sources_changed = true;
    }
    if ui.add_enabled(!app.has_active_adb_job(), egui::Button::new("Scan"))
        .on_hover_text("Scan configured sources on device")
        .clicked()
    {
        app.refresh_backup_source_scan();
    }
    if ui.add_enabled(!app.has_active_adb_job(), egui::Button::new("Analyze"))
        .on_hover_text("Analyze selected sources and calculate space")
        .clicked()
    {
        app.request_backup_analysis();
    }
    if app.backup_source_library.is_scanning {
        ui.spinner();
    }
});

// Destination row inside source library
ui.horizontal(|ui| {
    ui.label("Dest:");
    if ui.add(
        egui::TextEdit::singleline(&mut app.settings.destination_path)
            .desired_width(ui.available_width() - 40.0)
            .hint_text("Destination folder..."),
    ).changed() {
        app.invalidate_backup_analysis();
    }
    if ui.add_enabled(!app.has_active_adb_job(), egui::Button::new("..."))
        .on_hover_text("Browse for destination folder")
        .clicked()
    {
        app.pick_local_destination_folder();
    }
});
ui.add_space(4.0);
```

- [ ] **Step 3: Compact source cards with editable subfolder**

Replace each source card with compact layout. The subfolder is an inline editable `TextEdit`, not a read-only label:

```rust
Frame::new()
    .fill(Color32::from_rgb(250, 247, 240))
    .stroke(Stroke::new(1.0, Color32::from_rgb(228, 219, 203)))
    .corner_radius(CornerRadius::same(10))
    .inner_margin(Margin::same(8))
    .show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            if ui.checkbox(&mut source.enabled, "").changed() {
                backup_sources_changed = true;
            }
            if ui.add(
                egui::TextEdit::singleline(&mut source.label).desired_width(140.0),
            ).changed() {
                backup_sources_changed = true;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add_enabled(source_actions_enabled, egui::Button::new("✕"))
                    .on_hover_text("Remove source").clicked()
                {
                    backup_source_to_remove = Some(index);
                }
                if ui.add_enabled(source_actions_enabled, egui::Button::new("📁"))
                    .on_hover_text("Pick phone folder").clicked()
                {
                    backup_source_to_pick = Some(index);
                }
            });
        });

        // Source path (read-only display, full path on hover)
        ui.add(egui::Label::new(
            RichText::new(&source.source_path).small().color(Color32::from_rgb(118, 104, 85))
        ).wrap().truncate())
        .on_hover_text(format!("Source path: {}", source.source_path));

        // Editable destination subfolder inline
        ui.horizontal(|ui| {
            ui.small("→");
            if ui.add(
                egui::TextEdit::singleline(&mut source.destination_subfolder)
                    .desired_width(160.0)
                    .hint_text("subfolder"),
            ).changed() {
                backup_sources_changed = true;
            }
        });

        // Scan result inline
        if let Some(scan) = scan {
            if scan.exists {
                ui.small(format!("{} files | {}", scan.file_count, format_bytes(scan.total_bytes)));
            } else if let Some(error) = &scan.error {
                ui.colored_label(Color32::from_rgb(168, 52, 33), error);
            }
        }
    });
ui.add_space(4.0);
```

- [ ] **Step 4: Progress hover detail**

Replace the always-visible progress detail text with a hover tooltip:

```rust
let progress_bar = ui.add(
    egui::ProgressBar::new(total_progress)
        .text(format!("{} / {} files", app.progress.completed_files, app.progress.total_files))
        .fill(Color32::from_rgb(73, 121, 92)),
);
progress_bar.on_hover_text(progress_detail(&app.progress));
```

- [ ] **Step 5: Retry button as icon in file queue**

In the file queue table, replace the "Retry" text button with an icon:

```rust
// Replace: if ui.button("Retry").clicked()
// With:
if ui.button("↻").on_hover_text("Retry this file").clicked()
```

- [ ] **Step 6: Build and verify**

Run: `cargo build 2>&1`
Expected: compiles with no errors

- [ ] **Step 7: Commit**

```bash
git add src/ui/central_panel.rs
git commit -m "feat: compact central panel — scrollable, icon buttons, compact source cards, editable subfolders, hover details"
```

---

### Task 8: Tighten presets and header spacing

**Files:**
- Modify: `src/ui/side_panel.rs`
- Modify: `src/ui/widgets.rs`
- Modify: `src/ui/central_panel.rs`

- [ ] **Step 1: Tighter preset chip margins in widgets.rs**

In `render_preset_chip()`, change inner margin from `Margin::symmetric(10, 6)` to `Margin::symmetric(8, 5)`.

- [ ] **Step 2: Single-line save + preset name in side_panel.rs**

Replace the "Save Settings" / "Save as Preset" / text field section with:

```rust
ui.horizontal(|ui| {
    if ui.button(icon_or_text("💾", "Save"))
        .on_hover_text("Save current layout as preset")
        .clicked()
    {
        app.save_current_preset();
    }
    ui.add(
        egui::TextEdit::singleline(&mut app.preset_name_input)
            .desired_width(ui.available_width())
            .hint_text("Preset name"),
    );
});
```

- [ ] **Step 3: Tighten header spacing in central_panel.rs**

In `render_header()`, reduce vertical spacing slightly — reduce `add_space` calls by 2-4px where present. This is minor polish.

- [ ] **Step 4: Build and verify**

Run: `cargo build 2>&1`
Expected: compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add src/ui/side_panel.rs src/ui/widgets.rs src/ui/central_panel.rs
git commit -m "feat: tighter preset chips, single-line save control, header spacing"
```

---

### Task 9: Final build, version bump, push and release

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Bump version**

Change version in Cargo.toml from `"0.2.0"` to `"0.3.0"`.

- [ ] **Step 2: Release build**

Run: `cargo build --release 2>&1`
Expected: compiles with no errors and no warnings

- [ ] **Step 3: Fix any warnings**

If there are unused import warnings or dead code warnings, clean them up.

- [ ] **Step 4: Push all commits**

```bash
git push origin main
```

- [ ] **Step 5: Create GitHub release**

```bash
gh release create v0.3.0 target/release/adb-smart-backup.exe \
  --title "v0.3.0 - UI Overhaul" \
  --notes "## UI Overhaul

### Fixed
- Eliminated panel gap/black box rendering bug (fixed-width panel, no resize handle)

### Improved
- Both panels now scroll independently — no content clipped at small window sizes
- Compact device status: single row with hover details instead of full card
- Compact destination: single row with placeholder and browse button
- Validation section collapsed by default (click to expand)
- Cleanup tab compacted with placeholder text and icon buttons
- Run Controls (Start/Pause/Stop) pinned at bottom, always visible
- Source cards: checkbox + label + icons, editable subfolder inline, paths on hover
- Icon buttons throughout (+, Scan, Analyze, Browse, Remove, Pick Folder, Retry)
- Preset chips tighter with single-line save control

### Internal
- Decomposed app.rs (3000+ lines) into focused modules: ui/theme.rs, ui/widgets.rs, ui/side_panel.rs, ui/central_panel.rs"
```

- [ ] **Step 6: Verify release**

Run: `gh release view v0.3.0`
Expected: shows release with adb-smart-backup.exe asset attached
