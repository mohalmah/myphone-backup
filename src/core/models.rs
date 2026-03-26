use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeviceConnectionState {
    #[default]
    Disconnected,
    Connected,
    Unauthorized,
    Offline,
}

impl DeviceConnectionState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Disconnected => "NO DEVICE",
            Self::Connected => "CONNECTED",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::Offline => "OFFLINE",
        }
    }

    pub fn color(&self) -> eframe::egui::Color32 {
        match self {
            Self::Disconnected => eframe::egui::Color32::from_rgb(145, 92, 39),
            Self::Connected => eframe::egui::Color32::from_rgb(73, 121, 92),
            Self::Unauthorized => eframe::egui::Color32::from_rgb(168, 52, 33),
            Self::Offline => eframe::egui::Color32::from_rgb(124, 92, 161),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DeviceInfo {
    pub serial: String,
    pub model: Option<String>,
    pub state: DeviceConnectionState,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ValidationMode {
    Size,
    Md5,
}

impl ValidationMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Size => "File size",
            Self::Md5 => "MD5 hash",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExistingFileBehavior {
    Skip,
    Validate,
}

impl ExistingFileBehavior {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Skip => "Skip if name + size match",
            Self::Validate => "Validate before delete",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackupPreset {
    pub name: String,
    #[serde(default)]
    pub source_path: String,
    pub destination_path: String,
    #[serde(default)]
    pub sources: Vec<BackupSourceConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupSourceConfig {
    pub id: String,
    pub label: String,
    pub source_path: String,
    pub destination_subfolder: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub built_in: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub adb_path: String,
    pub source_path: String,
    pub destination_path: String,
    pub validation_mode: ValidationMode,
    pub existing_file_behavior: ExistingFileBehavior,
    pub auto_delete_after_success: bool,
    pub dry_run: bool,
    pub only_last_days: Option<u32>,
    #[serde(default = "default_backup_sources")]
    pub backup_sources: Vec<BackupSourceConfig>,
    pub presets: Vec<BackupPreset>,
}

impl Default for Settings {
    fn default() -> Self {
        let backup_sources = default_backup_sources();
        let presets = default_backup_presets();
        let default_preset = presets.first().cloned().unwrap_or_else(|| BackupPreset {
            name: "WhatsApp Essentials".to_string(),
            source_path: "/sdcard/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Video"
                .to_string(),
            destination_path: "E:\\AndroidBackups\\WhatsApp".to_string(),
            sources: vec![],
        });

        Self {
            adb_path: "adb".to_string(),
            source_path: default_preset.source_path.clone(),
            destination_path: default_preset.destination_path.clone(),
            validation_mode: ValidationMode::Size,
            existing_file_behavior: ExistingFileBehavior::Validate,
            auto_delete_after_success: false,
            dry_run: true,
            only_last_days: None,
            backup_sources,
            presets,
        }
    }
}

impl Settings {
    pub fn effective_backup_sources(&self) -> Vec<BackupSourceConfig> {
        let mut sources = self.backup_sources.clone();
        if sources.is_empty() && !self.source_path.trim().is_empty() {
            sources.push(legacy_source_from_path(
                &self.source_path,
                &guess_destination_subfolder(&self.source_path),
            ));
        }

        if sources.iter().all(|source| !source.enabled) && !sources.is_empty() {
            if let Some(first) = sources.first_mut() {
                first.enabled = true;
            }
        }

        sources
    }
}

#[derive(Clone, Debug)]
pub struct RemoteFile {
    pub name: String,
    pub remote_path: String,
    pub size_bytes: u64,
    pub modified_epoch_seconds: Option<i64>,
    pub source_root: String,
    pub source_label: String,
    pub destination_subfolder: String,
    pub relative_path: String,
}

#[derive(Clone, Debug)]
pub struct RemoteDirectory {
    pub name: String,
    pub full_path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteFolderEntryKind {
    Directory,
    File,
}

impl RemoteFolderEntryKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Directory => "Folder",
            Self::File => "File",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RemoteFolderEntry {
    pub full_path: String,
    pub kind: RemoteFolderEntryKind,
    pub size_bytes: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct RemoteFolderPreview {
    pub root_path: String,
    pub entries: Vec<RemoteFolderEntry>,
    pub file_count: usize,
    pub directory_count: usize,
    pub total_file_bytes: u64,
}

#[derive(Clone, Debug, Default)]
pub struct BackupPreflight {
    pub source_path: String,
    pub destination_path: String,
    pub total_files: usize,
    pub total_bytes: u64,
    pub files_to_copy: usize,
    pub bytes_to_copy: u64,
    pub matching_local_files: usize,
    pub conflicting_local_files: usize,
    pub destination_available_bytes: Option<u64>,
    pub destination_has_enough_space: bool,
    pub destination_space_error: Option<String>,
    pub system_drive_path: Option<String>,
    pub system_drive_available_bytes: Option<u64>,
    pub system_drive_warning: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct BackupAnalysis {
    pub files: Vec<RemoteFile>,
    pub source_summaries: Vec<BackupSourceScan>,
    pub preflight: BackupPreflight,
}

#[derive(Clone, Debug, Default)]
pub struct BackupSourceScan {
    pub id: String,
    pub label: String,
    pub source_path: String,
    pub destination_subfolder: String,
    pub enabled: bool,
    pub file_count: usize,
    pub total_bytes: u64,
    pub exists: bool,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileStatus {
    Queued,
    Copying,
    Validating,
    Deleted,
    Done,
    Skipped,
    Failed,
    Conflict,
    DryRun,
}

impl FileStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Copying => "Copying",
            Self::Validating => "Validating",
            Self::Deleted => "Deleted",
            Self::Done => "Done",
            Self::Skipped => "Skipped",
            Self::Failed => "Failed",
            Self::Conflict => "Conflict",
            Self::DryRun => "Dry Run",
        }
    }

    pub fn color(&self) -> eframe::egui::Color32 {
        match self {
            Self::Queued => eframe::egui::Color32::from_rgb(115, 95, 69),
            Self::Copying => eframe::egui::Color32::from_rgb(198, 106, 44),
            Self::Validating => eframe::egui::Color32::from_rgb(67, 102, 153),
            Self::Deleted => eframe::egui::Color32::from_rgb(168, 52, 33),
            Self::Done => eframe::egui::Color32::from_rgb(73, 121, 92),
            Self::Skipped => eframe::egui::Color32::from_rgb(115, 95, 69),
            Self::Failed => eframe::egui::Color32::from_rgb(168, 52, 33),
            Self::Conflict => eframe::egui::Color32::from_rgb(145, 92, 39),
            Self::DryRun => eframe::egui::Color32::from_rgb(124, 92, 161),
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Failed | Self::Conflict)
    }
}

#[derive(Clone, Debug)]
pub struct FileRecord {
    pub name: String,
    pub remote_path: String,
    pub local_path: PathBuf,
    pub size_bytes: u64,
    pub modified_epoch_seconds: Option<i64>,
    pub source_root: String,
    pub source_label: String,
    pub destination_subfolder: String,
    pub relative_path: String,
    pub status: FileStatus,
    pub detail: String,
    pub attempts: u8,
}

impl FileRecord {
    pub fn from_remote(remote: &RemoteFile, destination_root: &Path) -> Self {
        let mut local_path = destination_root.to_path_buf();
        if !remote.destination_subfolder.trim().is_empty() {
            local_path.push(&remote.destination_subfolder);
        }
        for segment in remote
            .relative_path
            .split('/')
            .filter(|segment| !segment.is_empty())
        {
            local_path.push(segment);
        }

        Self {
            name: remote.name.clone(),
            remote_path: remote.remote_path.clone(),
            local_path,
            size_bytes: remote.size_bytes,
            modified_epoch_seconds: remote.modified_epoch_seconds,
            source_root: remote.source_root.clone(),
            source_label: remote.source_label.clone(),
            destination_subfolder: remote.destination_subfolder.clone(),
            relative_path: remote.relative_path.clone(),
            status: FileStatus::Queued,
            detail: "Queued".to_string(),
            attempts: 0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SyncProgress {
    pub total_files: usize,
    pub completed_files: usize,
    pub failed_files: usize,
    pub total_bytes: u64,
    pub processed_bytes: u64,
    pub current_file: Option<String>,
    pub current_file_progress: f32,
    pub speed_bytes_per_sec: f64,
    pub eta_seconds: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct RunSummary {
    pub total_files: usize,
    pub copied: usize,
    pub deleted: usize,
    pub skipped: usize,
    pub failed: usize,
    pub conflicts: usize,
    pub dry_run_actions: usize,
    pub cancelled: bool,
}

fn default_true() -> bool {
    true
}

fn built_in_source(
    id: &str,
    label: &str,
    source_path: &str,
    destination_subfolder: &str,
    enabled: bool,
) -> BackupSourceConfig {
    BackupSourceConfig {
        id: id.to_string(),
        label: label.to_string(),
        source_path: source_path.to_string(),
        destination_subfolder: destination_subfolder.to_string(),
        enabled,
        built_in: true,
    }
}

pub fn default_backup_sources() -> Vec<BackupSourceConfig> {
    vec![
        built_in_source(
            "whatsapp-images",
            "WhatsApp Images",
            "/sdcard/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Images",
            "WhatsApp Images",
            true,
        ),
        built_in_source(
            "whatsapp-videos",
            "WhatsApp Videos",
            "/sdcard/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Video",
            "WhatsApp Videos",
            true,
        ),
        built_in_source(
            "whatsapp-documents",
            "WhatsApp Documents",
            "/sdcard/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Documents",
            "WhatsApp Documents",
            true,
        ),
        built_in_source(
            "whatsapp-audio",
            "WhatsApp Audio",
            "/sdcard/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Audio",
            "WhatsApp Audio",
            false,
        ),
        built_in_source(
            "downloads",
            "Downloads",
            "/sdcard/Download",
            "Downloads",
            false,
        ),
        built_in_source("camera", "Camera", "/sdcard/DCIM/Camera", "Camera", false),
        built_in_source(
            "telegram-images",
            "Telegram Images",
            "/sdcard/Telegram/Telegram Images",
            "Telegram Images",
            false,
        ),
        built_in_source(
            "telegram-video",
            "Telegram Video",
            "/sdcard/Telegram/Telegram Video",
            "Telegram Video",
            false,
        ),
    ]
}

pub fn default_backup_presets() -> Vec<BackupPreset> {
    vec![
        BackupPreset {
            name: "WhatsApp Essentials".to_string(),
            source_path: "/sdcard/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Video"
                .to_string(),
            destination_path: "E:\\AndroidBackups\\WhatsApp".to_string(),
            sources: default_backup_sources()
                .into_iter()
                .filter(|source| {
                    matches!(
                        source.id.as_str(),
                        "whatsapp-images" | "whatsapp-videos" | "whatsapp-documents"
                    )
                })
                .map(|mut source| {
                    source.enabled = true;
                    source
                })
                .collect(),
        },
        BackupPreset {
            name: "WhatsApp Full Media".to_string(),
            source_path: "/sdcard/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Video"
                .to_string(),
            destination_path: "E:\\AndroidBackups\\WhatsApp".to_string(),
            sources: default_backup_sources()
                .into_iter()
                .map(|mut source| {
                    source.enabled = source.id.starts_with("whatsapp");
                    source
                })
                .collect(),
        },
        BackupPreset {
            name: "Messaging Media".to_string(),
            source_path: "/sdcard/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Video"
                .to_string(),
            destination_path: "E:\\AndroidBackups\\Messaging".to_string(),
            sources: default_backup_sources()
                .into_iter()
                .map(|mut source| {
                    source.enabled = matches!(
                        source.id.as_str(),
                        "whatsapp-images"
                            | "whatsapp-videos"
                            | "whatsapp-documents"
                            | "telegram-images"
                            | "telegram-video"
                    );
                    source
                })
                .collect(),
        },
        BackupPreset {
            name: "Downloads".to_string(),
            source_path: "/sdcard/Download".to_string(),
            destination_path: "E:\\AndroidBackups\\Downloads".to_string(),
            sources: vec![built_in_source(
                "downloads",
                "Downloads",
                "/sdcard/Download",
                "Downloads",
                true,
            )],
        },
        BackupPreset {
            name: "Camera Roll".to_string(),
            source_path: "/sdcard/DCIM/Camera".to_string(),
            destination_path: "E:\\AndroidBackups\\Camera".to_string(),
            sources: vec![built_in_source(
                "camera",
                "Camera",
                "/sdcard/DCIM/Camera",
                "Camera",
                true,
            )],
        },
    ]
}

pub fn legacy_source_from_path(
    source_path: &str,
    destination_subfolder: &str,
) -> BackupSourceConfig {
    BackupSourceConfig {
        id: "custom-legacy".to_string(),
        label: destination_subfolder.to_string(),
        source_path: source_path.to_string(),
        destination_subfolder: destination_subfolder.to_string(),
        enabled: true,
        built_in: false,
    }
}

pub fn guess_destination_subfolder(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    normalized
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .unwrap_or("Backup Folder")
        .to_string()
}
