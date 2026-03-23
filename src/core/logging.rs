use anyhow::{Context, Result};
use chrono::Local;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
};

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

    pub fn log(&self, level: &str, message: &str) {
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(
                file,
                "[{}] [{}] {}",
                Local::now().format("%H:%M:%S"),
                level,
                message
            );
        }
    }
}
