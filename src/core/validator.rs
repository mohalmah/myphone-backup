use super::{
    adb::AdbController,
    models::{RemoteFile, ValidationMode},
};
use anyhow::{Context, Result, anyhow};
use std::{fs, path::Path, process::Command};

pub struct ValidationReport {
    pub passed: bool,
    pub detail: String,
}

pub fn validate_remote_vs_local(
    adb: &AdbController,
    remote_file: &RemoteFile,
    local_path: &Path,
    validation_mode: ValidationMode,
) -> Result<ValidationReport> {
    let local_metadata = fs::metadata(local_path)
        .with_context(|| format!("Missing local file {}", local_path.display()))?;
    let local_size = local_metadata.len();

    match validation_mode {
        ValidationMode::Size => {
            if local_size == remote_file.size_bytes {
                Ok(ValidationReport {
                    passed: true,
                    detail: format!("Validation passed (size match: {} bytes).", local_size),
                })
            } else {
                Ok(ValidationReport {
                    passed: false,
                    detail: format!(
                        "Validation failed (remote {} bytes vs local {} bytes).",
                        remote_file.size_bytes, local_size
                    ),
                })
            }
        }
        ValidationMode::Md5 => {
            let remote_md5 = adb.remote_md5(&remote_file.remote_path)?;
            let local_md5 = local_md5(local_path)?;

            if remote_md5 == local_md5 {
                Ok(ValidationReport {
                    passed: true,
                    detail: format!("Validation passed (MD5 {local_md5})."),
                })
            } else {
                Ok(ValidationReport {
                    passed: false,
                    detail: format!(
                        "Validation failed (remote MD5 {remote_md5} vs local MD5 {local_md5})."
                    ),
                })
            }
        }
    }
}

fn local_md5(path: &Path) -> Result<String> {
    let path = path.to_string_lossy().to_string();
    let output = Command::new("certutil")
        .args(["-hashfile", &path, "MD5"])
        .output()
        .with_context(|| format!("Failed to launch certutil for {}", path))?;

    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(anyhow!(if message.is_empty() {
            "certutil -hashfile failed".to_string()
        } else {
            message
        }));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find_map(extract_certutil_md5)
        .ok_or_else(|| anyhow!("Unable to parse certutil hash output for {}", path))
}

fn extract_certutil_md5(line: &str) -> Option<String> {
    let compact = line
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    if compact.len() == 32
        && compact
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        Some(compact.to_lowercase())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::extract_certutil_md5;

    #[test]
    fn parses_certutil_md5_line() {
        let hash = extract_certutil_md5("0c c1 75 b9 c0 f1 b6 a8 31 c3 99 e2 69 77 26 61");
        assert_eq!(hash.as_deref(), Some("0cc175b9c0f1b6a831c399e269772661"));
    }
}
