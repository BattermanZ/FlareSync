use crate::errors::FlareSyncError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct DomainStatus {
    pub last_checked_at: Option<String>,
    pub last_updated_at: Option<String>,
    pub last_status: String,
    pub last_error: Option<String>,
}

impl Default for DomainStatus {
    fn default() -> Self {
        Self {
            last_checked_at: None,
            last_updated_at: None,
            last_status: "pending".to_string(),
            last_error: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct RuntimeStatus {
    pub started_at: String,
    pub updated_at: String,
    pub last_public_ip: Option<String>,
    pub last_ip_check_at: Option<String>,
    pub domains: BTreeMap<String, DomainStatus>,
    pub last_error: Option<String>,
    pub shutting_down: bool,
}

impl RuntimeStatus {
    pub fn new() -> Self {
        let now = now_timestamp();
        Self {
            started_at: now.clone(),
            updated_at: now,
            last_public_ip: None,
            last_ip_check_at: None,
            domains: BTreeMap::new(),
            last_error: None,
            shutting_down: false,
        }
    }

    pub fn mark_ip_check_success(&mut self, ip: &Ipv4Addr) {
        let now = now_timestamp();
        self.updated_at = now.clone();
        self.last_public_ip = Some(ip.to_string());
        self.last_ip_check_at = Some(now);
        self.last_error = None;
    }

    pub fn mark_ip_check_error(&mut self, error: &FlareSyncError) {
        let now = now_timestamp();
        self.updated_at = now;
        self.last_error = Some(error.to_string());
    }

    pub fn mark_domain_result(&mut self, domain: &str, status: &str, updated: bool) {
        let now = now_timestamp();
        self.updated_at = now.clone();

        let domain_status = self.domains.entry(domain.to_string()).or_default();
        domain_status.last_checked_at = Some(now.clone());
        domain_status.last_status = status.to_string();
        if updated {
            domain_status.last_updated_at = Some(now);
        }
        domain_status.last_error = None;
        self.last_error = None;
    }

    pub fn mark_domain_error(&mut self, domain: &str, error: &FlareSyncError) {
        let now = now_timestamp();
        self.updated_at = now.clone();

        let domain_status = self.domains.entry(domain.to_string()).or_default();
        domain_status.last_checked_at = Some(now);
        domain_status.last_status = "error".to_string();
        domain_status.last_error = Some(error.to_string());
        self.last_error = Some(error.to_string());
    }

    pub fn mark_shutting_down(&mut self) {
        self.updated_at = now_timestamp();
        self.shutting_down = true;
    }

    pub fn write_to_path(&self, path: &Path) -> Result<(), FlareSyncError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        let json = serde_json::to_vec_pretty(self)?;
        let temp_path = temporary_status_path(path);

        let write_result = (|| -> Result<(), FlareSyncError> {
            let mut temp_file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)?;
            temp_file.write_all(&json)?;
            temp_file.sync_all()?;
            drop(temp_file);
            fs::rename(&temp_path, path)?;
            Ok(())
        })();

        if write_result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }

        write_result?;
        Ok(())
    }
}

impl Default for RuntimeStatus {
    fn default() -> Self {
        Self::new()
    }
}

fn now_timestamp() -> String {
    chrono::Local::now().to_rfc3339()
}

fn temporary_status_path(path: &Path) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("flaresync-status.json");
    path.with_file_name(format!(
        ".{}.{}.{}.tmp",
        file_name,
        std::process::id(),
        unique
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn test_runtime_status_records_ip_and_domain_success() {
        let mut status = RuntimeStatus::new();
        let ip: Ipv4Addr = "203.0.113.10".parse().unwrap();

        status.mark_ip_check_success(&ip);
        status.mark_domain_result("example.com", "updated", true);

        let domain = status.domains.get("example.com").unwrap();
        assert_eq!(status.last_public_ip, Some("203.0.113.10".to_string()));
        assert_eq!(domain.last_status, "updated");
        assert!(domain.last_checked_at.is_some());
        assert!(domain.last_updated_at.is_some());
        assert!(status.last_error.is_none());
    }

    #[test]
    fn test_runtime_status_records_missing_domain() {
        let mut status = RuntimeStatus::new();

        status.mark_domain_result("missing.example.com", "missing", false);

        let domain = status.domains.get("missing.example.com").unwrap();
        assert_eq!(domain.last_status, "missing");
        assert!(domain.last_checked_at.is_some());
        assert!(domain.last_updated_at.is_none());
        assert!(domain.last_error.is_none());
    }

    #[test]
    fn test_runtime_status_records_domain_error() {
        let mut status = RuntimeStatus::new();
        let error = FlareSyncError::Cloudflare("permission denied".to_string());

        status.mark_domain_error("example.com", &error);

        let domain = status.domains.get("example.com").unwrap();
        assert_eq!(domain.last_status, "error");
        assert!(domain
            .last_error
            .as_ref()
            .unwrap()
            .contains("permission denied"));
        assert!(status
            .last_error
            .as_ref()
            .unwrap()
            .contains("permission denied"));
    }

    #[test]
    fn test_runtime_status_writes_json_file() {
        let _guard = crate::test_support::global_lock();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let test_dir = std::env::temp_dir().join(format!(
            "flaresync_status_test_{}_{}",
            std::process::id(),
            unique
        ));
        let status_path = test_dir.join("nested").join("status.json");

        let mut status = RuntimeStatus::new();
        let ip: Ipv4Addr = "203.0.113.10".parse().unwrap();
        status.mark_ip_check_success(&ip);
        status.write_to_path(&status_path).unwrap();

        let written = fs::read_to_string(&status_path).unwrap();
        let value: Value = serde_json::from_str(&written).unwrap();

        assert_eq!(value["last_public_ip"], "203.0.113.10");
        assert!(value["started_at"].is_string());

        fs::remove_dir_all(test_dir).ok();
    }

    #[test]
    fn test_runtime_status_overwrites_json_file_atomically() {
        let _guard = crate::test_support::global_lock();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let test_dir = std::env::temp_dir().join(format!(
            "flaresync_status_atomic_test_{}_{}",
            std::process::id(),
            unique
        ));
        let status_path = test_dir.join("status.json");

        let mut first_status = RuntimeStatus::new();
        first_status.mark_domain_result("example.com", "missing", false);
        first_status.write_to_path(&status_path).unwrap();

        let mut second_status = RuntimeStatus::new();
        let ip: Ipv4Addr = "203.0.113.20".parse().unwrap();
        second_status.mark_ip_check_success(&ip);
        second_status.write_to_path(&status_path).unwrap();

        let written = fs::read_to_string(&status_path).unwrap();
        let value: Value = serde_json::from_str(&written).unwrap();
        assert_eq!(value["last_public_ip"], "203.0.113.20");

        let temp_files: Vec<_> = fs::read_dir(&test_dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .map(|name| name.ends_with(".tmp"))
                    .unwrap_or(false)
            })
            .collect();
        assert!(temp_files.is_empty());

        fs::remove_dir_all(test_dir).ok();
    }
}
