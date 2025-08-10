use dotenv::dotenv;
use log::{error, info, warn};
use reqwest::Client as ReqwestClient;
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::Path;
use std::time::Duration;
use tokio::time;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct DnsRecord {
    id: String,
    name: String,
    content: String,
    #[serde(rename = "type")]
    record_type: String,
    proxied: bool,
    ttl: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct CloudflareResponse<T> {
    success: bool,
    errors: Vec<serde_json::Value>,
    messages: Vec<serde_json::Value>,
    result: T,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    validate_env_vars()?;

    log4rs::init_file("/app/log4rs.yaml", Default::default())?;

    let api_token = env::var("CLOUDFLARE_API_TOKEN").expect("CLOUDFLARE_API_TOKEN must be set");
    let zone_id = env::var("CLOUDFLARE_ZONE_ID").expect("CLOUDFLARE_ZONE_ID must be set");
    let domain_names_str = env::var("DOMAIN_NAME").expect("DOMAIN_NAME must be set");
    let update_interval: u64 = env::var("UPDATE_INTERVAL")
        .expect("UPDATE_INTERVAL must be set")
        .parse()
        .expect("UPDATE_INTERVAL must be a number");

    let domain_names: Vec<String> = domain_names_str
        .split(|c| c == ',' || c == ';')
        .map(|s| s.trim().to_string())
        .collect();

    let client = ReqwestClient::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    info!("FlareSync started");

    loop {
        let current_ip = match get_current_ip(&client).await {
            Ok(ip) => ip,
            Err(e) => {
                error!("Failed to get current IP: {}. Retrying in 1 minute.", e);
                time::sleep(Duration::from_secs(60)).await;
                continue;
            }
        };
        info!("Current public IP: {}", current_ip);

        for domain_name in &domain_names {
            match check_and_update_ip(&client, &api_token, &zone_id, domain_name, &current_ip).await
            {
                Ok(updated) => {
                    if updated {
                        info!("IP address updated successfully for {}", domain_name);
                    } else {
                        info!("No update needed for {}", domain_name);
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to check or update IP for {}: {}",
                        domain_name, e
                    );
                }
            }
        }

        info!("Waiting for {} minutes before next check", update_interval);
        time::sleep(Duration::from_secs(update_interval * 60)).await;
    }
}

async fn get_current_ip(client: &ReqwestClient) -> Result<Ipv4Addr, Box<dyn Error>> {
    let ip_str = retry_with_backoff(|| client.get("https://api.ipify.org").send())
        .await?
        .text()
        .await?;
    ip_str.parse().map_err(|e| e.into())
}

async fn check_and_update_ip(
    client: &ReqwestClient,
    api_token: &str,
    zone_id: &str,
    domain_name: &str,
    current_ip: &Ipv4Addr,
) -> Result<bool, Box<dyn Error>> {
    info!("Checking DNS for domain: {}", domain_name);
    let dns_records: CloudflareResponse<Vec<DnsRecord>> = retry_with_backoff(|| {
        client
            .get(&format!(
                "https://api.cloudflare.com/client/v4/zones/{}/dns_records?type=A&name={}",
                zone_id, domain_name
            ))
            .header("Authorization", format!("Bearer {}", api_token))
            .header("Content-Type", "application/json")
            .send()
    })
    .await?
    .json()
    .await?;

    if let Some(record) = dns_records.result.get(0) {
        info!(
            "Current Cloudflare DNS record IP for {}: {}",
            domain_name, record.content
        );

        if record.content != current_ip.to_string() {
            info!("IP for {} has changed. Updating DNS record...", domain_name);

            backup_dns_record(record, domain_name)?;

            let update_response: CloudflareResponse<DnsRecord> = retry_with_backoff(|| {
                client
                    .put(&format!(
                        "https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}",
                        zone_id, record.id
                    ))
                    .header("Authorization", format!("Bearer {}", api_token))
                    .header("Content-Type", "application/json")
                    .json(&serde_json::json!({
                        "type": "A",
                        "name": domain_name,
                        "content": current_ip.to_string(),
                        "ttl": record.ttl,
                        "proxied": record.proxied
                    }))
                    .send()
            })
            .await?
            .json()
            .await?;

            if update_response.success {
                info!("DNS record for {} updated successfully!", domain_name);
                Ok(true)
            } else {
                error!(
                    "Failed to update DNS record for {}: {:?}",
                    domain_name, update_response.errors
                );
                Err(format!("Failed to update DNS record for {}", domain_name).into())
            }
        } else {
            info!("IP for {} hasn't changed. No update needed.", domain_name);
            Ok(false)
        }
    } else {
        warn!("No matching DNS record found for {}.", domain_name);
        Ok(false)
    }
}

async fn retry_with_backoff<T, F, Fut>(f: F) -> Result<T, Box<dyn Error>>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, reqwest::Error>>,
{
    let mut retries = 0;
    let max_retries = 3;
    let mut wait_time = Duration::from_secs(1);

    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if retries >= max_retries {
                    return Err(e.into());
                }
                error!("Request failed: {}. Retrying in {:?}...", e, wait_time);
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

fn backup_dns_record(record: &DnsRecord, domain_name: &str) -> Result<(), Box<dyn Error>> {
    let backup_dir = Path::new("/app/backups");
    if !backup_dir.exists() {
        fs::create_dir(backup_dir)?;
    }

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let filename = format!("{}_{}_backup.json", timestamp, domain_name);
    let backup_path = backup_dir.join(filename);

    let mut file = File::create(backup_path)?;
    let json = serde_json::to_string_pretty(record)?;
    file.write_all(json.as_bytes())?;

    info!("DNS record backup created successfully");
    Ok(())
}

fn validate_env_vars() -> Result<(), Box<dyn Error>> {
    let required_vars = vec!["CLOUDFLARE_API_TOKEN", "CLOUDFLARE_ZONE_ID", "DOMAIN_NAME", "UPDATE_INTERVAL"];

    for var in required_vars {
        if env::var(var).is_err() {
            return Err(format!("Environment variable {} is not set", var).into());
        }
    }

    Ok(())
}

