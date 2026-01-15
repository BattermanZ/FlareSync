use crate::errors::FlareSyncError;
use log::{info, warn};
use reqwest::Client as ReqwestClient;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, File};
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::Path;
use std::time::Duration;
use tokio::time;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DnsRecord {
    pub id: String,
    pub name: String,
    pub content: String,
    #[serde(rename = "type")]
    pub record_type: String,
    pub proxied: bool,
    pub ttl: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CloudflareResponse<T> {
    pub success: bool,
    pub errors: Vec<serde_json::Value>,
    pub messages: Vec<serde_json::Value>,
    pub result: T,
}

fn is_transient_cloudflare_error(err: &FlareSyncError) -> bool {
    match err {
        FlareSyncError::CloudflareTransient(_) => true,
        FlareSyncError::Network(e) => match e.status() {
            Some(status) => status.as_u16() == 429 || status.is_server_error(),
            None => true,
        },
        _ => false,
    }
}

fn cloudflare_errors_look_transient(errors: &[Value]) -> bool {
    errors.iter().any(|error| {
        let code = error.get("code").and_then(|v| v.as_i64());
        if code == Some(1015) {
            return true;
        }

        let message = error
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        message.contains("rate limit")
            || message.contains("ratelimit")
            || message.contains("too many requests")
            || message.contains("temporar")
            || message.contains("timeout")
            || message.contains("try again")
    })
}

async fn retry_cloudflare<T, F, Fut>(mut f: F) -> Result<T, FlareSyncError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, FlareSyncError>>,
{
    let mut retries = 0;
    let max_retries = 3;
    let mut wait_time = Duration::from_secs(1);

    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if !is_transient_cloudflare_error(&e) || retries >= max_retries {
                    return Err(e);
                }
                warn!(
                    "Cloudflare request failed: {}. Retrying in {:?}...",
                    e, wait_time
                );
                time::sleep(wait_time).await;
                retries += 1;
                wait_time *= 2;
                if wait_time > Duration::from_secs(60) {
                    wait_time = Duration::from_secs(60);
                }
            }
        }
    }
}

fn sanitize_filename_component(input: &str) -> String {
    let mut sanitized: String = input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();

    const MAX_LEN: usize = 128;
    if sanitized.len() > MAX_LEN {
        sanitized.truncate(MAX_LEN);
    }
    if sanitized.is_empty() {
        sanitized = "record".to_string();
    }
    sanitized
}

async fn get_dns_record(
    client: &ReqwestClient,
    api_token: &str,
    zone_id: &str,
    domain_name: &str,
) -> Result<Option<DnsRecord>, FlareSyncError> {
    let response: CloudflareResponse<Vec<DnsRecord>> = retry_cloudflare(|| async {
        let resp = client
            .get(format!(
                "https://api.cloudflare.com/client/v4/zones/{}/dns_records",
                zone_id
            ))
            .query(&[("type", "A"), ("name", domain_name)])
            .header("Authorization", format!("Bearer {}", api_token))
            .header("Content-Type", "application/json")
            .send()
            .await?
            .error_for_status()?;
        let response: CloudflareResponse<Vec<DnsRecord>> = resp.json().await?;
        if response.success {
            Ok(response)
        } else if cloudflare_errors_look_transient(&response.errors) {
            Err(FlareSyncError::CloudflareTransient(format!(
                "API error (transient) fetching {}: {:?}",
                domain_name, response.errors
            )))
        } else {
            Err(FlareSyncError::Cloudflare(format!(
                "API error fetching {}: {:?}",
                domain_name, response.errors
            )))
        }
    })
    .await?;

    Ok(response.result.into_iter().next())
}

async fn update_dns_record(
    client: &ReqwestClient,
    api_token: &str,
    zone_id: &str,
    record: &DnsRecord,
    current_ip: &Ipv4Addr,
) -> Result<(), FlareSyncError> {
    let _response: CloudflareResponse<DnsRecord> = retry_cloudflare(|| async {
        let resp = client
            .put(format!(
                "https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}",
                zone_id, record.id
            ))
            .header("Authorization", format!("Bearer {}", api_token))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "type": "A",
                "name": record.name,
                "content": current_ip.to_string(),
                "ttl": record.ttl,
                "proxied": record.proxied
            }))
            .send()
            .await?
            .error_for_status()?;
        let response: CloudflareResponse<DnsRecord> = resp.json().await?;
        if response.success {
            Ok(response)
        } else if cloudflare_errors_look_transient(&response.errors) {
            Err(FlareSyncError::CloudflareTransient(format!(
                "API error (transient) updating {}: {:?}",
                record.name, response.errors
            )))
        } else {
            Err(FlareSyncError::Cloudflare(format!(
                "API error updating {}: {:?}",
                record.name, response.errors
            )))
        }
    })
    .await?;

    info!("DNS record for {} updated successfully!", record.name);
    Ok(())
}

fn backup_dns_record(record: &DnsRecord) -> Result<(), FlareSyncError> {
    let backup_dir = Path::new("backups");
    fs::create_dir_all(backup_dir)?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let safe_name = sanitize_filename_component(&record.name);
    let filename = format!("{}_{}_backup.json", timestamp, safe_name);
    let backup_path = backup_dir.join(filename);

    let mut file = File::create(backup_path)?;
    let json = serde_json::to_string_pretty(record)?;
    file.write_all(json.as_bytes())?;

    info!("DNS record backup created for {}", record.name);
    Ok(())
}

pub async fn check_and_update_ip(
    client: &ReqwestClient,
    api_token: &str,
    zone_id: &str,
    domain_name: &str,
    current_ip: &Ipv4Addr,
) -> Result<bool, FlareSyncError> {
    info!("Checking DNS for domain: {}", domain_name);

    if let Some(record) = get_dns_record(client, api_token, zone_id, domain_name).await? {
        info!(
            "Current Cloudflare DNS record IP for {}: {}",
            domain_name, record.content
        );

        if record.content != current_ip.to_string() {
            info!("IP for {} has changed. Updating DNS record...", domain_name);
            backup_dns_record(&record)?;
            update_dns_record(client, api_token, zone_id, &record, current_ip).await?;
            Ok(true)
        } else {
            info!("IP for {} hasn't changed. No update needed.", domain_name);
            Ok(false)
        }
    } else {
        warn!("No matching DNS record found for {}.", domain_name);
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn test_backup_dns_record() {
        let _guard = crate::test_support::global_lock();

        let record = DnsRecord {
            id: "1".to_string(),
            name: "test.com".to_string(),
            content: "127.0.0.1".to_string(),
            record_type: "A".to_string(),
            proxied: false,
            ttl: 120,
        };

        // Create a temporary directory for the test
        let test_dir = Path::new("target/test_output");
        fs::create_dir_all(test_dir).unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(test_dir).unwrap();

        let result = backup_dns_record(&record);
        assert!(result.is_ok());

        let backup_dir = Path::new("backups");
        assert!(backup_dir.exists());

        let mut found = false;
        for entry in fs::read_dir(backup_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_file() && path.to_str().unwrap().contains("test.com_backup.json") {
                let content = fs::read_to_string(path).unwrap();
                let backed_up_record: DnsRecord = serde_json::from_str(&content).unwrap();
                assert_eq!(backed_up_record.id, record.id);
                found = true;
                break;
            }
        }

        // Cleanup
        std::env::set_current_dir(original_cwd).unwrap();
        fs::remove_dir_all(test_dir).unwrap();

        assert!(found, "Backup file was not found");
    }

    #[test]
    fn test_sanitize_filename_component() {
        let _guard = crate::test_support::global_lock();

        assert_eq!(
            sanitize_filename_component("example.com"),
            "example.com".to_string()
        );
        assert_eq!(
            sanitize_filename_component("../weird/name"),
            ".._weird_name".to_string()
        );
    }
}
