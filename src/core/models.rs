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
    pub source_path: String,
    pub destination_path: String,
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
    pub presets: Vec<BackupPreset>,
}

impl Default for Settings {
    fn default() -> Self {
        let default_preset = BackupPreset {
            name: "WhatsApp Videos".to_string(),
            source_path: "/sdcard/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Video"
                .to_string(),
            destination_path: "E:\\Xiaomi12TPro\\24-03-2026 before Misk\\Whatsapp Video"
                .to_string(),
        };

        Self {
            adb_path: "adb".to_string(),
            source_path: default_preset.source_path.clone(),
            destination_path: default_preset.destination_path.clone(),
            validation_mode: ValidationMode::Size,
            existing_file_behavior: ExistingFileBehavior::Validate,
            auto_delete_after_success: false,
            dry_run: true,
            only_last_days: None,
            presets: vec![default_preset],
        }
    }
}

#[derive(Clone, Debug)]
pub struct RemoteFile {
    pub name: String,
    pub remote_path: String,
    pub size_bytes: u64,
    pub modified_epoch_seconds: Option<i64>,
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
    pub status: FileStatus,
    pub detail: String,
    pub attempts: u8,
}

impl FileRecord {
    pub fn from_remote(remote: &RemoteFile, destination_root: &Path) -> Self {
        Self {
            name: remote.name.clone(),
            remote_path: remote.remote_path.clone(),
            local_path: destination_root.join(&remote.name),
            size_bytes: remote.size_bytes,
            modified_epoch_seconds: remote.modified_epoch_seconds,
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
