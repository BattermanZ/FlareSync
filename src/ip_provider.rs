use crate::errors::FlareSyncError;
use log::error;
use reqwest::Client as ReqwestClient;
use std::net::Ipv4Addr;
use std::time::Duration;
use tokio::time;

async fn retry_with_backoff<T, F, Fut>(f: F) -> Result<T, FlareSyncError>
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

pub async fn get_current_ip(client: &ReqwestClient) -> Result<Ipv4Addr, FlareSyncError> {
    let ip_str = retry_with_backoff(|| client.get("https://api.ipify.org").send())
        .await?
        .text()
        .await?;
    ip_str.parse::<Ipv4Addr>().map_err(|_|
        FlareSyncError::Cloudflare(format!("Failed to parse IP address: {}", ip_str))
    )
}
