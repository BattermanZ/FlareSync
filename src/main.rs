use log::{error, info};
use reqwest::Client as ReqwestClient;
use std::time::Duration;
use tokio::time;

use flaresync::config::Config;
use flaresync::cloudflare::check_and_update_ip;
use flaresync::ip_provider::get_current_ip;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log_config_path = std::env::var("LOG_CONFIG_PATH").unwrap_or_else(|_| "log4rs.yaml".to_string());
    log4rs::init_file(&log_config_path, Default::default())?;

    let config = Config::from_env()?;

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

        for domain_name in &config.domain_names {
            match check_and_update_ip(&client, &config.api_token, &config.zone_id, domain_name, &current_ip).await
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

        info!("Waiting for {:?} before next check", config.update_interval);
        time::sleep(config.update_interval).await;
    }
}