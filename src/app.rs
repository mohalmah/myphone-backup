use crate::core::{
    config,
    models::{
        BackupPreset, DeviceConnectionState, DeviceInfo, ExistingFileBehavior, FileRecord,
        RemoteDirectory, RemoteFile, RemoteFolderEntryKind, RemoteFolderPreview, RunSummary,
        Settings, SyncProgress, ValidationMode,
    },
    sync::{self, SyncEvent, SyncHandle, SyncPlan},
};
use chrono::Local;
use eframe::egui::{
    self, Align, Color32, Context, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea,
    Stroke,
};
use rfd::FileDialog;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum RemoteFolderPickerTarget {
    #[default]
    SourceFolder,
    CleanupFolder,
}

#[derive(Default)]
struct RemoteFolderPicker {
    is_open: bool,
    is_loading: bool,
    current_path: String,
    target: RemoteFolderPickerTarget,
    entries: Vec<RemoteDirectory>,
    receiver: Option<Receiver<Result<Vec<RemoteDirectory>, String>>>,
    error: Option<String>,
}

#[derive(Default)]
struct FolderCleanupState {
    folder_path: String,
    preview: Option<RemoteFolderPreview>,
    preview_receiver: Option<Receiver<Result<RemoteFolderPreview, String>>>,
    delete_receiver: Option<Receiver<Result<String, String>>>,
    is_fetching_preview: bool,
    is_deleting: bool,
    preview_error: Option<String>,
    delete_error: Option<String>,
    delete_armed: bool,
}

pub struct BackupApp {
    settings: Settings,
    device_info: DeviceInfo,
    device_probe_receiver: Option<Receiver<Result<DeviceInfo, String>>>,
    sync_receiver: Option<Receiver<SyncEvent>>,
    sync_handle: Option<SyncHandle>,
    remote_folder_picker: RemoteFolderPicker,
    folder_cleanup: FolderCleanupState,
    files: Vec<FileRecord>,
    progress: SyncProgress,
    log_lines: Vec<String>,
    last_summary: Option<RunSummary>,
    selected_preset_name: String,
    preset_name_input: String,
    status_banner: String,
    error_banner: Option<String>,
}

impl BackupApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_theme(&cc.egui_ctx);

        let settings = config::load_settings().unwrap_or_default();
        let initial_cleanup_folder = settings.source_path.clone();
        let selected_preset_name = settings
            .presets
            .first()
            .map(|preset| preset.name.clone())
            .unwrap_or_else(|| "WhatsApp Videos".to_string());

        let mut app = Self {
            preset_name_input: selected_preset_name.clone(),
            selected_preset_name,
            settings,
            device_info: DeviceInfo::default(),
            device_probe_receiver: None,
            sync_receiver: None,
            sync_handle: None,
            remote_folder_picker: RemoteFolderPicker::default(),
            folder_cleanup: FolderCleanupState {
                folder_path: initial_cleanup_folder,
                ..Default::default()
            },
            files: Vec::new(),
            progress: SyncProgress::default(),
            log_lines: Vec::new(),
            last_summary: None,
            status_banner: "Ready to scan your Android device.".to_string(),
            error_banner: None,
        };

        if config::settings_path().exists() {
            app.push_log("[INFO] Loaded settings from config/settings.json");
        } else {
            app.push_log("[INFO] Using default WhatsApp backup preset");
        }

        app.refresh_device_info();
        app
    }

    fn push_log(&mut self, line: impl Into<String>) {
        self.log_lines.push(line.into());
        if self.log_lines.len() > 300 {
            let excess = self.log_lines.len() - 300;
            self.log_lines.drain(0..excess);
        }
    }

    fn is_running(&self) -> bool {
        self.sync_receiver.is_some()
    }

    fn refresh_device_info(&mut self) {
        self.status_banner = "Checking ADB connection...".to_string();
        self.device_probe_receiver = Some(sync::start_device_probe(self.settings.adb_path.clone()));
    }

    fn pick_local_destination_folder(&mut self) {
        let initial_directory = initial_local_directory(&self.settings.destination_path);
        if let Some(folder) = FileDialog::new()
            .set_directory(initial_directory)
            .pick_folder()
        {
            let selected = folder.to_string_lossy().to_string();
            self.settings.destination_path = selected.clone();
            self.status_banner = format!("Selected local destination folder: {selected}");
            self.push_log(format!("[INFO] Local destination folder selected: {selected}"));
        }
    }

    fn open_source_folder_picker(&mut self) {
        let start_path = normalize_remote_path(&self.settings.source_path);
        self.open_remote_folder_picker(RemoteFolderPickerTarget::SourceFolder, start_path);
    }

    fn open_cleanup_folder_picker(&mut self) {
        let seed_path = if self.folder_cleanup.folder_path.trim().is_empty() {
            self.settings.source_path.clone()
        } else {
            self.folder_cleanup.folder_path.clone()
        };
        let start_path = normalize_remote_path(&seed_path);
        self.open_remote_folder_picker(RemoteFolderPickerTarget::CleanupFolder, start_path);
    }

    fn open_remote_folder_picker(
        &mut self,
        target: RemoteFolderPickerTarget,
        start_path: String,
    ) {
        self.remote_folder_picker.target = target;
        self.remote_folder_picker.is_open = true;
        self.request_remote_directory_listing(start_path);
    }

    fn request_remote_directory_listing(&mut self, path: String) {
        let normalized_path = normalize_remote_path(&path);
        self.remote_folder_picker.current_path = normalized_path.clone();
        self.remote_folder_picker.entries.clear();
        self.remote_folder_picker.error = None;
        self.remote_folder_picker.is_loading = true;
        self.remote_folder_picker.receiver = Some(sync::start_remote_directory_list(
            self.settings.adb_path.clone(),
            normalized_path,
        ));
    }

    fn poll_remote_folder_picker(&mut self) {
        let mut outcome = None;
        let mut disconnected = false;

        if let Some(receiver) = &self.remote_folder_picker.receiver {
            match receiver.try_recv() {
                Ok(result) => outcome = Some(result),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => disconnected = true,
            }
        }

        if let Some(result) = outcome {
            self.remote_folder_picker.receiver = None;
            self.remote_folder_picker.is_loading = false;
            match result {
                Ok(entries) => {
                    self.remote_folder_picker.entries = entries;
                    self.remote_folder_picker.error = None;
                }
                Err(error) => {
                    self.remote_folder_picker.error = Some(error.clone());
                    self.push_log(format!("[ERROR] {error}"));
                }
            }
        } else if disconnected {
            self.remote_folder_picker.receiver = None;
            self.remote_folder_picker.is_loading = false;
        }
    }

    fn set_cleanup_folder_path(&mut self, path: String) {
        let normalized = normalize_remote_path(&path);
        if self.folder_cleanup.folder_path != normalized {
            self.folder_cleanup.folder_path = normalized;
            self.folder_cleanup.preview = None;
            self.folder_cleanup.preview_error = None;
            self.folder_cleanup.delete_error = None;
            self.folder_cleanup.delete_armed = false;
        }
    }

    fn request_cleanup_preview(&mut self) {
        if self.folder_cleanup.folder_path.trim().is_empty() {
            self.folder_cleanup.preview_error =
                Some("Choose a phone folder before fetching contents.".to_string());
            return;
        }

        let path = normalize_remote_path(&self.folder_cleanup.folder_path);
        self.folder_cleanup.folder_path = path.clone();
        self.folder_cleanup.preview = None;
        self.folder_cleanup.preview_error = None;
        self.folder_cleanup.delete_error = None;
        self.folder_cleanup.delete_armed = false;
        self.folder_cleanup.is_fetching_preview = true;
        self.folder_cleanup.preview_receiver = Some(sync::start_remote_folder_preview(
            self.settings.adb_path.clone(),
            path.clone(),
        ));
        self.status_banner = format!("Fetching folder contents for {path}...");
        self.push_log(format!("[INFO] Fetching cleanup preview for {path}"));
    }

    fn request_cleanup_delete(&mut self) {
        let path = normalize_remote_path(&self.folder_cleanup.folder_path);

        if let Some(reason) = protected_cleanup_folder_reason(&path) {
            self.folder_cleanup.delete_error = Some(reason.to_string());
            return;
        }

        let preview_matches_path = self
            .folder_cleanup
            .preview
            .as_ref()
            .map(|preview| preview.root_path == path)
            .unwrap_or(false);

        if !preview_matches_path {
            self.folder_cleanup.delete_error =
                Some("Fetch folder contents before deleting.".to_string());
            return;
        }

        if !self.folder_cleanup.delete_armed {
            self.folder_cleanup.delete_error = Some(
                "Arm deletion first by checking the confirmation box.".to_string(),
            );
            return;
        }

        self.folder_cleanup.delete_error = None;
        self.folder_cleanup.is_deleting = true;
        self.folder_cleanup.delete_receiver = Some(sync::start_remote_folder_delete(
            self.settings.adb_path.clone(),
            path.clone(),
        ));
        self.status_banner = format!("Deleting remote folder {path}...");
        self.push_log(format!("[INFO] Deleting remote folder recursively: {path}"));
    }

    fn poll_cleanup_jobs(&mut self) {
        let mut preview_outcome = None;
        let mut preview_disconnected = false;

        if let Some(receiver) = &self.folder_cleanup.preview_receiver {
            match receiver.try_recv() {
                Ok(result) => preview_outcome = Some(result),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => preview_disconnected = true,
            }
        }

        if let Some(result) = preview_outcome {
            self.folder_cleanup.preview_receiver = None;
            self.folder_cleanup.is_fetching_preview = false;
            match result {
                Ok(preview) => {
                    let message = format!(
                        "Fetched cleanup preview: {} files, {} folders.",
                        preview.file_count, preview.directory_count
                    );
                    self.folder_cleanup.preview_error = None;
                    self.folder_cleanup.preview = Some(preview);
                    self.status_banner = message.clone();
                    self.push_log(format!("[INFO] {message}"));
                }
                Err(error) => {
                    self.folder_cleanup.preview = None;
                    self.folder_cleanup.preview_error = Some(error.clone());
                    self.status_banner = "Failed to fetch folder contents.".to_string();
                    self.push_log(format!("[ERROR] {error}"));
                }
            }
        } else if preview_disconnected {
            self.folder_cleanup.preview_receiver = None;
            self.folder_cleanup.is_fetching_preview = false;
        }

        let mut delete_outcome = None;
        let mut delete_disconnected = false;

        if let Some(receiver) = &self.folder_cleanup.delete_receiver {
            match receiver.try_recv() {
                Ok(result) => delete_outcome = Some(result),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => delete_disconnected = true,
            }
        }

        if let Some(result) = delete_outcome {
            self.folder_cleanup.delete_receiver = None;
            self.folder_cleanup.is_deleting = false;
            self.folder_cleanup.delete_armed = false;
            match result {
                Ok(path) => {
                    self.folder_cleanup.preview = None;
                    self.folder_cleanup.preview_error = None;
                    self.folder_cleanup.delete_error = None;
                    self.status_banner = format!("Deleted remote folder: {path}");
                    self.push_log(format!(
                        "[INFO] Deleted remote folder and all contents: {path}"
                    ));
                }
                Err(error) => {
                    self.folder_cleanup.delete_error = Some(error.clone());
                    self.status_banner = "Remote folder delete failed.".to_string();
                    self.push_log(format!("[ERROR] {error}"));
                }
            }
        } else if delete_disconnected {
            self.folder_cleanup.delete_receiver = None;
            self.folder_cleanup.is_deleting = false;
        }
    }

    fn save_settings(&mut self) {
        match config::save_settings(&self.settings) {
            Ok(()) => {
                self.status_banner = "Settings saved to config/settings.json".to_string();
                self.push_log("[INFO] Saved settings to config/settings.json");
            }
            Err(error) => {
                let message = format!("Failed to save settings: {error}");
                self.error_banner = Some(message.clone());
                self.push_log(format!("[ERROR] {message}"));
            }
        }
    }

    fn save_current_preset(&mut self) {
        let name = if self.preset_name_input.trim().is_empty() {
            format!("Preset {}", Local::now().format("%Y-%m-%d %H:%M"))
        } else {
            self.preset_name_input.trim().to_string()
        };

        let preset = BackupPreset {
            name: name.clone(),
            source_path: self.settings.source_path.clone(),
            destination_path: self.settings.destination_path.clone(),
        };

        if let Some(existing) = self
            .settings
            .presets
            .iter_mut()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(&name))
        {
            *existing = preset;
        } else {
            self.settings.presets.push(preset);
            self.settings
                .presets
                .sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        }

        self.selected_preset_name = name.clone();
        self.preset_name_input = name.clone();
        self.save_settings();
        self.status_banner = format!("Preset \"{name}\" saved.");
    }

    fn load_selected_preset(&mut self) {
        let selected = self.selected_preset_name.clone();
        if let Some(preset) = self
            .settings
            .presets
            .iter()
            .find(|candidate| candidate.name == selected)
            .cloned()
        {
            self.settings.source_path = preset.source_path;
            self.settings.destination_path = preset.destination_path;
            self.preset_name_input = preset.name.clone();
            self.status_banner = format!("Loaded preset \"{}\".", preset.name);
            self.push_log(format!("[INFO] Loaded preset \"{}\"", preset.name));
        }
    }

    fn start_sync(&mut self, plan: SyncPlan, reset_state: bool) {
        if self.is_running() {
            return;
        }

        self.save_settings();
        self.error_banner = None;

        if reset_state {
            self.files.clear();
            self.progress = SyncProgress::default();
            self.last_summary = None;
            self.log_lines.clear();
            self.push_log("[INFO] Starting backup run");
        }

        let session = sync::start_sync(self.settings.clone(), plan);
        self.sync_receiver = Some(session.receiver);
        self.sync_handle = Some(session.handle);
        self.status_banner = "Backup in progress...".to_string();
    }

    fn start_full_backup(&mut self) {
        self.start_sync(SyncPlan::FullScan, true);
    }

    fn start_retry(&mut self, file: RemoteFile) {
        self.push_log(format!("[INFO] Retrying {}", file.name));
        self.start_sync(SyncPlan::RetrySingle(file), false);
    }

    fn upsert_file(&mut self, record: FileRecord) {
        if let Some(existing) = self
            .files
            .iter_mut()
            .find(|candidate| candidate.remote_path == record.remote_path)
        {
            *existing = record;
        } else {
            self.files.push(record);
            self.files
                .sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        }
    }

    fn handle_sync_event(&mut self, event: SyncEvent) -> bool {
        match event {
            SyncEvent::LogLine(line) => self.push_log(line),
            SyncEvent::Device(info) => self.device_info = info,
            SyncEvent::FileUpdated(record) => self.upsert_file(record),
            SyncEvent::Progress(progress) => self.progress = progress,
            SyncEvent::FatalError(message) => {
                self.error_banner = Some(message.clone());
                self.push_log(format!("[ERROR] {message}"));
            }
            SyncEvent::Finished(summary) => {
                self.last_summary = Some(summary.clone());
                if summary.cancelled {
                    self.status_banner = "Run stopped.".to_string();
                } else if summary.failed > 0 || summary.conflicts > 0 {
                    self.status_banner = "Run completed with issues.".to_string();
                } else {
                    self.status_banner = "Run completed successfully.".to_string();
                }
                return true;
            }
        }

        false
    }

    fn poll_device_probe(&mut self) {
        let mut outcome = None;
        let mut disconnected = false;

        if let Some(receiver) = &self.device_probe_receiver {
            match receiver.try_recv() {
                Ok(result) => outcome = Some(result),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => disconnected = true,
            }
        }

        if let Some(result) = outcome {
            self.device_probe_receiver = None;
            match result {
                Ok(info) => {
                    self.device_info = info.clone();
                    self.status_banner = match info.state {
                        DeviceConnectionState::Connected => {
                            "Device connected. Ready to back up.".to_string()
                        }
                        DeviceConnectionState::Unauthorized => {
                            "Device found, but ADB authorization is pending.".to_string()
                        }
                        DeviceConnectionState::Offline => {
                            "Device detected, but ADB reports it as offline.".to_string()
                        }
                        DeviceConnectionState::Disconnected => {
                            "No Android device detected.".to_string()
                        }
                    };
                }
                Err(error) => {
                    self.error_banner = Some(error.clone());
                    self.status_banner = "ADB is not ready yet.".to_string();
                    self.push_log(format!("[ERROR] {error}"));
                }
            }
        } else if disconnected {
            self.device_probe_receiver = None;
        }
    }

    fn poll_sync_events(&mut self) {
        let mut events = Vec::new();
        let mut disconnected = false;

        if let Some(receiver) = &self.sync_receiver {
            loop {
                match receiver.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }

        let mut finished = false;
        for event in events {
            finished |= self.handle_sync_event(event);
        }

        if finished || disconnected {
            self.sync_receiver = None;
            self.sync_handle = None;
        }
    }

    fn show_remote_folder_picker(&mut self, ctx: &Context) {
        if !self.remote_folder_picker.is_open {
            return;
        }

        let current_path = self.remote_folder_picker.current_path.clone();
        let picker_target = self.remote_folder_picker.target;
        let entries = self.remote_folder_picker.entries.clone();
        let error = self.remote_folder_picker.error.clone();
        let is_loading = self.remote_folder_picker.is_loading;
        let can_go_up = parent_remote_path(&current_path).is_some();
        let mut window_open = self.remote_folder_picker.is_open;
        let mut navigate_to = None;
        let mut select_current = false;
        let mut refresh_listing = false;
        let mut go_up = false;

        egui::Window::new(match picker_target {
            RemoteFolderPickerTarget::SourceFolder => "Select Backup Source Folder",
            RemoteFolderPickerTarget::CleanupFolder => "Select Folder For Cleanup",
        })
            .open(&mut window_open)
            .collapsible(false)
            .resizable(true)
            .default_size([620.0, 420.0])
            .show(ctx, |ui| {
                ui.label("Browse directories on the connected Android device");
                ui.add_space(6.0);
                ui.label(RichText::new(&current_path).monospace());
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(can_go_up && !is_loading, egui::Button::new("Up"))
                        .clicked()
                    {
                        go_up = true;
                    }

                    if ui
                        .add_enabled(!is_loading, egui::Button::new("Refresh"))
                        .clicked()
                    {
                        refresh_listing = true;
                    }

                    if ui.button("Use This Folder").clicked() {
                        select_current = true;
                    }
                });

                ui.add_space(10.0);

                if is_loading {
                    ui.spinner();
                    ui.label("Loading folders from device...");
                } else if let Some(error) = &error {
                    ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                } else if entries.is_empty() {
                    ui.label("No subfolders found here. You can still use the current folder.");
                }

                ScrollArea::vertical()
                    .max_height(260.0)
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        for directory in &entries {
                            let label = format!("[Dir] {}", directory.name);
                            if ui
                                .add_enabled(!is_loading, egui::Button::new(label))
                                .clicked()
                            {
                                navigate_to = Some(directory.full_path.clone());
                            }
                            ui.small(RichText::new(&directory.full_path).monospace());
                            ui.add_space(6.0);
                        }
                    });
            });

        self.remote_folder_picker.is_open = window_open;

        if let Some(path) = navigate_to {
            self.request_remote_directory_listing(path);
        } else if go_up {
            if let Some(parent) = parent_remote_path(&current_path) {
                self.request_remote_directory_listing(parent);
            }
        } else if refresh_listing {
            self.request_remote_directory_listing(current_path.clone());
        } else if select_current {
            match picker_target {
                RemoteFolderPickerTarget::SourceFolder => {
                    self.settings.source_path = current_path.clone();
                    self.status_banner = format!("Selected phone source folder: {current_path}");
                    self.push_log(format!("[INFO] Phone source folder selected: {current_path}"));
                }
                RemoteFolderPickerTarget::CleanupFolder => {
                    self.set_cleanup_folder_path(current_path.clone());
                    self.status_banner = format!("Selected cleanup folder: {current_path}");
                    self.push_log(format!("[INFO] Cleanup folder selected: {current_path}"));
                }
            }
            self.remote_folder_picker.is_open = false;
        }
    }
}

impl eframe::App for BackupApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.poll_device_probe();
        self.poll_sync_events();
        self.poll_remote_folder_picker();
        self.poll_cleanup_jobs();

        if self.sync_receiver.is_some()
            || self.device_probe_receiver.is_some()
            || self.remote_folder_picker.receiver.is_some()
            || self.folder_cleanup.preview_receiver.is_some()
            || self.folder_cleanup.delete_receiver.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(200));
        }

        egui::TopBottomPanel::top("hero").show(ctx, |ui| {
            Frame::new()
                .fill(Color32::from_rgb(241, 234, 218))
                .inner_margin(Margin::same(18))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new("ADB Smart Backup & Cleanup")
                                    .heading()
                                    .strong()
                                    .color(Color32::from_rgb(50, 43, 34)),
                            );
                            ui.label(
                                RichText::new(
                                    "Safe per-file backup, validation, and optional cleanup for Android media folders.",
                                )
                                .color(Color32::from_rgb(93, 81, 66)),
                            );
                        });

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            status_pill(
                                ui,
                                if self.is_running() { "RUNNING" } else { "IDLE" },
                                if self.is_running() {
                                    Color32::from_rgb(198, 106, 44)
                                } else {
                                    Color32::from_rgb(73, 121, 92)
                                },
                            );
                            status_pill(
                                ui,
                                self.device_info.state.label(),
                                self.device_info.state.color(),
                            );
                        });
                    });
                    ui.add_space(10.0);
                    ui.label(RichText::new(&self.status_banner).color(Color32::from_rgb(72, 62, 50)));
                    if let Some(error) = &self.error_banner {
                        ui.add_space(8.0);
                        ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                    }
                });
        });

        egui::SidePanel::left("settings_panel")
            .resizable(true)
            .default_width(360.0)
            .show(ctx, |ui| {
                ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        settings_card(ui, "Connection", |ui| {
                            ui.label("ADB executable");
                            ui.text_edit_singleline(&mut self.settings.adb_path);
                            ui.add_space(8.0);
                            if ui.button("Refresh Device").clicked() {
                                self.refresh_device_info();
                            }

                            ui.add_space(10.0);
                            ui.label(RichText::new("Current Device").strong());
                            ui.label(device_summary(&self.device_info));
                        });

                        settings_card(ui, "Folders", |ui| {
                            ui.label("Phone source folder");
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(&mut self.settings.source_path);
                                if ui
                                    .add_enabled(
                                        !self.is_running(),
                                        egui::Button::new("Select Phone Folder..."),
                                    )
                                    .clicked()
                                {
                                    self.open_source_folder_picker();
                                }
                            });
                            ui.small(
                                "Phone selection uses the built-in ADB folder browser so the app keeps a valid /sdcard/... path.",
                            );
                            ui.add_space(8.0);
                            ui.label("Local destination folder");
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(&mut self.settings.destination_path);
                                if ui.button("Select Windows Folder...").clicked() {
                                    self.pick_local_destination_folder();
                                }
                            });
                            ui.small("Opens the normal Windows folder picker.");
                        });

                        settings_card(ui, "Folder Cleanup", |ui| {
                            ui.label("Phone folder to delete recursively");
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(&mut self.folder_cleanup.folder_path);
                                if ui
                                    .add_enabled(
                                        !self.folder_cleanup.is_fetching_preview
                                            && !self.folder_cleanup.is_deleting,
                                        egui::Button::new("Select Phone Folder..."),
                                    )
                                    .clicked()
                                {
                                    self.open_cleanup_folder_picker();
                                }
                            });
                            ui.small(
                                "Fetch contents first, then delete the selected phone folder and everything inside it.",
                            );

                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(
                                        !self.folder_cleanup.is_fetching_preview
                                            && !self.folder_cleanup.is_deleting,
                                        egui::Button::new("Fetch Contents"),
                                    )
                                    .clicked()
                                {
                                    self.request_cleanup_preview();
                                }

                                if ui.button("Clear Preview").clicked() {
                                    self.folder_cleanup.preview = None;
                                    self.folder_cleanup.preview_error = None;
                                    self.folder_cleanup.delete_error = None;
                                    self.folder_cleanup.delete_armed = false;
                                }
                            });

                            if let Some(reason) = protected_cleanup_folder_reason(
                                &self.folder_cleanup.folder_path,
                            ) {
                                ui.colored_label(Color32::from_rgb(168, 52, 33), reason);
                            }

                            if self.folder_cleanup.is_fetching_preview {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label("Fetching folder contents from device...");
                                });
                            }

                            if let Some(preview) = &self.folder_cleanup.preview {
                                ui.label(RichText::new(cleanup_summary(preview)).strong());
                            }

                            if let Some(error) = &self.folder_cleanup.preview_error {
                                ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                            }
                            if let Some(error) = &self.folder_cleanup.delete_error {
                                ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                            }

                            ui.add_space(8.0);
                            ui.checkbox(
                                &mut self.folder_cleanup.delete_armed,
                                "I understand this permanently deletes the selected phone folder and all its contents",
                            );

                            let preview_matches_path = self
                                .folder_cleanup
                                .preview
                                .as_ref()
                                .map(|preview| {
                                    preview.root_path
                                        == normalize_remote_path(&self.folder_cleanup.folder_path)
                                })
                                .unwrap_or(false);
                            let delete_allowed = preview_matches_path
                                && self.folder_cleanup.delete_armed
                                && !self.folder_cleanup.is_fetching_preview
                                && !self.folder_cleanup.is_deleting
                                && protected_cleanup_folder_reason(
                                    &self.folder_cleanup.folder_path,
                                )
                                .is_none();

                            if ui
                                .add_enabled(
                                    delete_allowed,
                                    egui::Button::new("Delete Folder + Contents"),
                                )
                                .clicked()
                            {
                                self.request_cleanup_delete();
                            }

                            if self.folder_cleanup.is_deleting {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label("Deleting folder recursively on device...");
                                });
                            }
                        });

                        settings_card(ui, "Validation", |ui| {
                            egui::ComboBox::from_label("Validation mode")
                                .selected_text(self.settings.validation_mode.label())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.settings.validation_mode,
                                        ValidationMode::Size,
                                        ValidationMode::Size.label(),
                                    );
                                    ui.selectable_value(
                                        &mut self.settings.validation_mode,
                                        ValidationMode::Md5,
                                        ValidationMode::Md5.label(),
                                    );
                                });

                            egui::ComboBox::from_label("Existing local files")
                                .selected_text(self.settings.existing_file_behavior.label())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.settings.existing_file_behavior,
                                        ExistingFileBehavior::Skip,
                                        ExistingFileBehavior::Skip.label(),
                                    );
                                    ui.selectable_value(
                                        &mut self.settings.existing_file_behavior,
                                        ExistingFileBehavior::Validate,
                                        ExistingFileBehavior::Validate.label(),
                                    );
                                });

                            ui.checkbox(
                                &mut self.settings.auto_delete_after_success,
                                "Auto delete on device after successful validation",
                            );
                            ui.checkbox(&mut self.settings.dry_run, "Dry-run mode (simulate only)");

                            let mut filter_recent = self.settings.only_last_days.is_some();
                            if ui
                                .checkbox(&mut filter_recent, "Copy only recent files")
                                .changed()
                            {
                                self.settings.only_last_days =
                                    if filter_recent { Some(7) } else { None };
                            }
                            if let Some(days) = &mut self.settings.only_last_days {
                                ui.horizontal(|ui| {
                                    ui.label("Days");
                                    ui.add(egui::DragValue::new(days).range(1..=365));
                                });
                            }
                        });

                        settings_card(ui, "Presets", |ui| {
                            ui.label("Preset name");
                            ui.text_edit_singleline(&mut self.preset_name_input);
                            ui.horizontal(|ui| {
                                if ui.button("Save Current Preset").clicked() {
                                    self.save_current_preset();
                                }
                                if ui.button("Load Selected").clicked() {
                                    self.load_selected_preset();
                                }
                            });
                            egui::ComboBox::from_label("Saved presets")
                                .selected_text(if self.selected_preset_name.is_empty() {
                                    "Select a preset".to_string()
                                } else {
                                    self.selected_preset_name.clone()
                                })
                                .show_ui(ui, |ui| {
                                    for preset in &self.settings.presets {
                                        ui.selectable_value(
                                            &mut self.selected_preset_name,
                                            preset.name.clone(),
                                            preset.name.clone(),
                                        );
                                    }
                                });
                            if ui.button("Save Settings").clicked() {
                                self.save_settings();
                            }
                        });

                        settings_card(ui, "Run Controls", |ui| {
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(
                                        !self.is_running(),
                                        egui::Button::new("Start Backup"),
                                    )
                                    .clicked()
                                {
                                    self.start_full_backup();
                                }

                                let paused = self
                                    .sync_handle
                                    .as_ref()
                                    .map(SyncHandle::is_paused)
                                    .unwrap_or(false);
                                if ui
                                    .add_enabled(
                                        self.is_running(),
                                        egui::Button::new(if paused { "Resume" } else { "Pause" }),
                                    )
                                    .clicked()
                                {
                                    if let Some(handle) = &self.sync_handle {
                                        handle.toggle_pause();
                                    }
                                }

                                if ui
                                    .add_enabled(self.is_running(), egui::Button::new("Stop"))
                                    .clicked()
                                {
                                    if let Some(handle) = &self.sync_handle {
                                        handle.request_stop();
                                    }
                                }
                            });

                            ui.add_space(8.0);
                            ui.label(
                                "Deletion is always per-file and only attempted after validation passes.",
                            );
                        });
                    });
            });

        egui::TopBottomPanel::bottom("log_panel")
            .resizable(true)
            .default_height(180.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Activity Log").strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{} lines", self.log_lines.len()))
                                .color(Color32::from_rgb(118, 104, 85)),
                        );
                    });
                });
                ui.add_space(6.0);
                ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        for line in self.log_lines.iter().rev() {
                            ui.monospace(line);
                        }
                    });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let total_progress = if self.progress.total_files == 0 {
                0.0
            } else {
                self.progress.completed_files as f32 / self.progress.total_files as f32
            };

            summary_strip(ui, &self.progress, self.last_summary.as_ref());

            Frame::new()
                .fill(Color32::from_rgb(250, 247, 240))
                .stroke(Stroke::new(1.0, Color32::from_rgb(221, 211, 190)))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::same(14))
                .show(ui, |ui| {
                    ui.label(RichText::new("Progress").strong());
                    ui.add_space(8.0);
                    ui.add(
                        egui::ProgressBar::new(total_progress)
                            .text(format!(
                                "{} / {} files complete",
                                self.progress.completed_files, self.progress.total_files
                            ))
                            .fill(Color32::from_rgb(73, 121, 92)),
                    );
                    ui.add_space(6.0);
                    ui.add(
                        egui::ProgressBar::new(self.progress.current_file_progress)
                            .text(match &self.progress.current_file {
                                Some(current_file) => format!("Current file: {current_file}"),
                                None => "Waiting to start".to_string(),
                            })
                            .fill(Color32::from_rgb(198, 106, 44)),
                    );
                    ui.add_space(8.0);
                    ui.label(progress_detail(&self.progress));
                });

            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("File Queue").strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} files tracked", self.files.len()))
                            .color(Color32::from_rgb(118, 104, 85)),
                    );
                });
            });
            ui.add_space(8.0);

            let mut retry_target = None;

            Frame::new()
                .fill(Color32::WHITE)
                .stroke(Stroke::new(1.0, Color32::from_rgb(221, 211, 190)))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::same(14))
                .show(ui, |ui| {
                    ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            egui::Grid::new("file_grid")
                                .striped(true)
                                .num_columns(5)
                                .min_col_width(110.0)
                                .spacing([12.0, 8.0])
                                .show(ui, |ui| {
                                    ui.label(RichText::new("File Name").strong());
                                    ui.label(RichText::new("Size").strong());
                                    ui.label(RichText::new("Status").strong());
                                    ui.label(RichText::new("Details").strong());
                                    ui.label(RichText::new("Action").strong());
                                    ui.end_row();

                                    for record in &self.files {
                                        ui.label(&record.name);
                                        ui.label(format_bytes(record.size_bytes));
                                        ui.colored_label(
                                            record.status.color(),
                                            record.status.label(),
                                        );
                                        ui.label(&record.detail);

                                        if record.status.is_retryable() && !self.is_running() {
                                            if ui.button("Retry").clicked() {
                                                retry_target = Some(RemoteFile {
                                                    name: record.name.clone(),
                                                    remote_path: record.remote_path.clone(),
                                                    size_bytes: record.size_bytes,
                                                    modified_epoch_seconds: record
                                                        .modified_epoch_seconds,
                                                });
                                            }
                                        } else {
                                            ui.label("-");
                                        }
                                        ui.end_row();
                                    }
                                });

                            if self.files.is_empty() {
                                ui.add_space(12.0);
                                ui.label(
                                    "No files scanned yet. Start a run to populate the queue.",
                                );
                            }
                        });
                });

            if self.folder_cleanup.preview.is_some()
                || self.folder_cleanup.is_fetching_preview
                || self.folder_cleanup.preview_error.is_some()
                || self.folder_cleanup.delete_error.is_some()
            {
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Folder Cleanup Preview").strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(preview) = &self.folder_cleanup.preview {
                            ui.label(
                                RichText::new(format!("{} entries", preview.entries.len()))
                                    .color(Color32::from_rgb(118, 104, 85)),
                            );
                        }
                    });
                });
                ui.add_space(8.0);

                Frame::new()
                    .fill(Color32::WHITE)
                    .stroke(Stroke::new(1.0, Color32::from_rgb(221, 211, 190)))
                    .corner_radius(CornerRadius::same(14))
                    .inner_margin(Margin::same(14))
                    .show(ui, |ui| {
                        if self.folder_cleanup.is_fetching_preview {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Fetching cleanup preview...");
                            });
                        }

                        if let Some(error) = &self.folder_cleanup.preview_error {
                            ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                        }
                        if let Some(error) = &self.folder_cleanup.delete_error {
                            ui.colored_label(Color32::from_rgb(168, 52, 33), error);
                        }

                        if let Some(preview) = &self.folder_cleanup.preview {
                            ui.label(RichText::new(cleanup_summary(preview)).strong());
                            ui.add_space(8.0);

                            ScrollArea::vertical()
                                .max_height(260.0)
                                .auto_shrink([false; 2])
                                .show(ui, |ui| {
                                    egui::Grid::new("cleanup_preview_grid")
                                        .striped(true)
                                        .num_columns(3)
                                        .min_col_width(120.0)
                                        .spacing([12.0, 8.0])
                                        .show(ui, |ui| {
                                            ui.label(RichText::new("Type").strong());
                                            ui.label(RichText::new("Size").strong());
                                            ui.label(RichText::new("Path").strong());
                                            ui.end_row();

                                            for entry in &preview.entries {
                                                ui.label(entry.kind.label());
                                                ui.label(match entry.kind {
                                                    RemoteFolderEntryKind::Directory => "-".to_string(),
                                                    RemoteFolderEntryKind::File => format_bytes(
                                                        entry.size_bytes.unwrap_or(0),
                                                    ),
                                                });
                                                ui.label(RichText::new(&entry.full_path).monospace());
                                                ui.end_row();
                                            }
                                        });
                                });
                        }
                    });
            }

            if let Some(remote_file) = retry_target {
                self.start_retry(remote_file);
            }
        });

        self.show_remote_folder_picker(ctx);
    }
}

fn apply_theme(ctx: &Context) {
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = Color32::from_rgb(247, 241, 230);
    visuals.extreme_bg_color = Color32::from_rgb(255, 252, 246);
    visuals.override_text_color = Some(Color32::from_rgb(51, 43, 35));
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(248, 243, 236);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(244, 238, 227);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(234, 225, 207);
    visuals.widgets.active.bg_fill = Color32::from_rgb(225, 213, 188);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(66, 56, 45));
    visuals.window_fill = Color32::from_rgb(247, 241, 230);
    visuals.selection.bg_fill = Color32::from_rgb(198, 106, 44);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.interact_size = egui::vec2(44.0, 28.0);
    ctx.set_style(style);
}

fn settings_card(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    Frame::new()
        .fill(Color32::from_rgb(250, 247, 240))
        .stroke(Stroke::new(1.0, Color32::from_rgb(221, 211, 190)))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.label(RichText::new(title).strong());
            ui.add_space(8.0);
            add_contents(ui);
        });
    ui.add_space(12.0);
}

fn status_pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    Frame::new()
        .fill(color.gamma_multiply(0.16))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.45)))
        .corner_radius(CornerRadius::same(255))
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.colored_label(color, RichText::new(text).strong());
        });
}

fn device_summary(info: &DeviceInfo) -> String {
    match info.state {
        DeviceConnectionState::Connected => match &info.model {
            Some(model) => format!("{model}\nSerial: {}\n{}", info.serial, info.message),
            None => format!("Serial: {}\n{}", info.serial, info.message),
        },
        DeviceConnectionState::Unauthorized
        | DeviceConnectionState::Offline
        | DeviceConnectionState::Disconnected => info.message.clone(),
    }
}

fn summary_strip(ui: &mut egui::Ui, progress: &SyncProgress, summary: Option<&RunSummary>) {
    ui.horizontal_wrapped(|ui| {
        metric_chip(
            ui,
            "Total Files",
            progress.total_files.to_string(),
            Color32::from_rgb(73, 121, 92),
        );
        metric_chip(
            ui,
            "Processed",
            progress.completed_files.to_string(),
            Color32::from_rgb(198, 106, 44),
        );
        metric_chip(
            ui,
            "Speed",
            format!(
                "{}/s",
                format_bytes(progress.speed_bytes_per_sec.round() as u64)
            ),
            Color32::from_rgb(67, 102, 153),
        );
        metric_chip(
            ui,
            "ETA",
            progress
                .eta_seconds
                .map(format_duration)
                .unwrap_or_else(|| "n/a".to_string()),
            Color32::from_rgb(124, 92, 161),
        );
    });
    ui.add_space(10.0);

    if let Some(summary) = summary {
        ui.horizontal_wrapped(|ui| {
            metric_chip(
                ui,
                "Copied",
                summary.copied.to_string(),
                Color32::from_rgb(73, 121, 92),
            );
            metric_chip(
                ui,
                "Deleted",
                summary.deleted.to_string(),
                Color32::from_rgb(168, 52, 33),
            );
            metric_chip(
                ui,
                "Skipped",
                summary.skipped.to_string(),
                Color32::from_rgb(115, 95, 69),
            );
            metric_chip(
                ui,
                "Failed",
                summary.failed.to_string(),
                Color32::from_rgb(168, 52, 33),
            );
            metric_chip(
                ui,
                "Conflicts",
                summary.conflicts.to_string(),
                Color32::from_rgb(145, 92, 39),
            );
        });
    }
}

fn metric_chip(ui: &mut egui::Ui, label: &str, value: String, color: Color32) {
    Frame::new()
        .fill(color.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.45)))
        .corner_radius(CornerRadius::same(255))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(color, RichText::new(label).strong());
                ui.label(value);
            });
        });
}

fn progress_detail(progress: &SyncProgress) -> String {
    let current = progress
        .current_file
        .clone()
        .unwrap_or_else(|| "None".to_string());
    format!(
        "Current file: {current} | Total data: {} | Processed data: {} | Failed files: {}",
        format_bytes(progress.total_bytes),
        format_bytes(progress.processed_bytes),
        progress.failed_files
    )
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

    if bytes == 0 {
        return "0 B".to_string();
    }

    let mut value = bytes as f64;
    let mut unit_index = 0usize;
    while value >= 1024.0 && unit_index < UNITS.len() - 1 {
        value /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{bytes} {}", UNITS[unit_index])
    } else {
        format!("{value:.1} {}", UNITS[unit_index])
    }
}

fn format_duration(seconds: f64) -> String {
    let rounded = seconds.round().max(0.0) as u64;
    let hours = rounded / 3600;
    let minutes = (rounded % 3600) / 60;
    let secs = rounded % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

fn cleanup_summary(preview: &RemoteFolderPreview) -> String {
    format!(
        "Root: {} | {} files | {} folders | {} total file data",
        preview.root_path,
        preview.file_count,
        preview.directory_count,
        format_bytes(preview.total_file_bytes)
    )
}

fn protected_cleanup_folder_reason(path: &str) -> Option<&'static str> {
    if path.trim().is_empty() {
        return Some("Choose a phone folder first.");
    }

    let normalized = normalize_remote_path(path);
    match normalized.as_str() {
        "/" => Some("Deleting the root folder is blocked."),
        "/sdcard" => Some("Deleting /sdcard is blocked."),
        "/storage" => Some("Deleting /storage is blocked."),
        "/storage/emulated" => Some("Deleting /storage/emulated is blocked."),
        "/storage/emulated/0" => Some("Deleting /storage/emulated/0 is blocked."),
        "/sdcard/Android" => Some("Deleting /sdcard/Android is blocked."),
        "/sdcard/Android/media" => Some("Deleting /sdcard/Android/media is blocked."),
        _ => None,
    }
}

fn initial_local_directory(path: &str) -> PathBuf {
    let candidate = PathBuf::from(path);
    if candidate.is_dir() {
        return candidate;
    }

    if let Some(parent) = candidate.parent() {
        if parent.is_dir() {
            return parent.to_path_buf();
        }
    }

    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return "/sdcard".to_string();
    }

    let normalized = trimmed.replace('\\', "/");
    if normalized == "/" {
        "/".to_string()
    } else {
        normalized.trim_end_matches('/').to_string()
    }
}

fn parent_remote_path(path: &str) -> Option<String> {
    let normalized = normalize_remote_path(path);
    if normalized == "/" {
        return None;
    }

    let trimmed = normalized.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) => Some("/".to_string()),
        Some(index) => Some(trimmed[..index].to_string()),
        None => None,
    }
}
