use crate::errors::FlareSyncError;
use log::{info, warn, error};
use reqwest::Client as ReqwestClient;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::Path;

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

async fn get_dns_record(
    client: &ReqwestClient,
    api_token: &str,
    zone_id: &str,
    domain_name: &str,
) -> Result<Option<DnsRecord>, FlareSyncError> {
    let response: CloudflareResponse<Vec<DnsRecord>> = client
        .get(&format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records?type=A&name={}",
            zone_id, domain_name
        ))
        .header("Authorization", format!("Bearer {}", api_token))
        .header("Content-Type", "application/json")
        .send()
        .await?
        .json()
        .await?;

    if !response.success {
        return Err(FlareSyncError::Cloudflare(format!(
            "API error: {:?}",
            response.errors
        )));
    }

    Ok(response.result.into_iter().next())
}

async fn update_dns_record(
    client: &ReqwestClient,
    api_token: &str,
    zone_id: &str,
    record: &DnsRecord,
    current_ip: &Ipv4Addr,
) -> Result<(), FlareSyncError> {
    let response: CloudflareResponse<DnsRecord> = client
        .put(&format!(
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
        .json()
        .await?;

    if response.success {
        info!("DNS record for {} updated successfully!", record.name);
        Ok(())
    } else {
        error!(
            "Failed to update DNS record for {}: {:?}",
            record.name, response.errors
        );
        Err(FlareSyncError::Cloudflare(format!(
            "Failed to update DNS record for {}",
            record.name
        )))
    }
}

fn backup_dns_record(record: &DnsRecord) -> Result<(), FlareSyncError> {
    let backup_dir = Path::new("backups");
    fs::create_dir_all(backup_dir)?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let filename = format!("{}_{}_backup.json", timestamp, record.name);
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
}