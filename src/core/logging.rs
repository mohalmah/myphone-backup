use anyhow::{Context, Result};
use chrono::Local;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
};

const MAX_DETAIL_CHARS: usize = 16_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Error,
    Trace,
}

impl LogLevel {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Error => "ERROR",
            Self::Trace => "TRACE",
        }
    }
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
    pub detail: Option<String>,
    pub detailed_only: bool,
}

impl LogEntry {
    pub fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            timestamp: Local::now().format("%H:%M:%S").to_string(),
            level,
            message: message.into(),
            detail: None,
            detailed_only: false,
        }
    }

    pub fn with_detail(
        level: LogLevel,
        message: impl Into<String>,
        detail: impl Into<String>,
        detailed_only: bool,
    ) -> Self {
        Self {
            timestamp: Local::now().format("%H:%M:%S").to_string(),
            level,
            message: message.into(),
            detail: sanitize_detail(detail.into()),
            detailed_only,
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Info, message)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Error, message)
    }

    pub fn trace(message: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::with_detail(LogLevel::Trace, message, detail, true)
    }

    pub fn compact_line(&self) -> String {
        format!(
            "[{}] [{}] {}",
            self.timestamp,
            self.level.label(),
            self.message
        )
    }

    pub fn from_legacy_line(line: impl Into<String>) -> Self {
        let line = line.into();
        if let Some(message) = line.strip_prefix("[INFO] ") {
            return Self::info(message.to_string());
        }
        if let Some(message) = line.strip_prefix("[ERROR] ") {
            return Self::error(message.to_string());
        }
        if let Some(message) = line.strip_prefix("[TRACE] ") {
            return Self::trace("Detailed activity", message.to_string());
        }
        Self::info(line)
    }
}

fn sanitize_detail(detail: String) -> Option<String> {
    let detail = detail.trim().to_string();
    if detail.is_empty() {
        return None;
    }

    if detail.chars().count() <= MAX_DETAIL_CHARS {
        return Some(detail);
    }

    let truncated = detail.chars().take(MAX_DETAIL_CHARS).collect::<String>();
    Some(format!(
        "{truncated}\n\n... detail truncated after {MAX_DETAIL_CHARS} characters ..."
    ))
}

pub struct AppLogger {
    file: Mutex<File>,
}

impl AppLogger {
    pub fn new(log_directory: PathBuf) -> Result<Self> {
        fs::create_dir_all(&log_directory)
            .with_context(|| format!("Failed to create {}", log_directory.display()))?;
        let file_path = log_directory.join(format!("{}.txt", Local::now().format("%Y-%m-%d")));
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .with_context(|| format!("Failed to open {}", file_path.display()))?;

        Ok(Self {
            file: Mutex::new(file),
        })
    }

    pub fn log_entry(&self, entry: &LogEntry) {
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(file, "{}", entry.compact_line());
            if let Some(detail) = &entry.detail {
                let _ = writeln!(file, "    ----------------");
                for line in detail.lines() {
                    let _ = writeln!(file, "    {}", line);
                }
                let _ = writeln!(file, "    ----------------");
            }
        }
    }
}
