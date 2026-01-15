use crate::errors::FlareSyncError;
use log::error;
use reqwest::Client as ReqwestClient;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::time::Duration;
use tokio::time;

const IP_SOURCES: [&str; 3] = [
    "https://api.ipify.org",
    "https://checkip.amazonaws.com",
    "https://ipv4.icanhazip.com",
];

async fn fetch_ipv4_from_source(
    client: &ReqwestClient,
    url: &'static str,
) -> Result<Ipv4Addr, FlareSyncError> {
    let mut retries = 0;
    let max_retries = 3;
    let mut wait_time = Duration::from_secs(1);
    let per_attempt_timeout = Duration::from_secs(10);

    loop {
        let response: Result<reqwest::Response, FlareSyncError> =
            match time::timeout(per_attempt_timeout, client.get(url).send()).await {
                Ok(result) => result.map_err(FlareSyncError::from),
                Err(_) => Err(FlareSyncError::IpProvider(format!(
                    "Timed out fetching IP from {}",
                    url
                ))),
            };

        match response {
            Ok(resp) => {
                let resp = resp.error_for_status()?;
                let body = time::timeout(per_attempt_timeout, resp.text())
                    .await
                    .map_err(|_| {
                        FlareSyncError::IpProvider(format!(
                            "Timed out reading response from {}",
                            url
                        ))
                    })??;
                let ip_str = body.trim();
                return ip_str.parse::<Ipv4Addr>().map_err(|_| {
                    FlareSyncError::IpProvider(format!(
                        "Failed to parse IPv4 address from {}: {}",
                        url, ip_str
                    ))
                });
            }
            Err(e) => {
                let transient = matches!(e, FlareSyncError::Network(_))
                    || matches!(
                        e,
                        FlareSyncError::IpProvider(ref s) if s.contains("Timed out")
                    );
                if !transient || retries >= max_retries {
                    return Err(e);
                }
                error!(
                    "IP source request failed: {}. Retrying in {:?}...",
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

pub async fn get_current_ip(client: &ReqwestClient) -> Result<Ipv4Addr, FlareSyncError> {
    let (r1, r2, r3) = tokio::join!(
        fetch_ipv4_from_source(client, IP_SOURCES[0]),
        fetch_ipv4_from_source(client, IP_SOURCES[1]),
        fetch_ipv4_from_source(client, IP_SOURCES[2]),
    );

    let mut counts: HashMap<Ipv4Addr, usize> = HashMap::new();
    for result in [r1, r2, r3] {
        if let Ok(ip) = result {
            *counts.entry(ip).or_insert(0) += 1;
        }
    }

    if let Some((ip, count)) = counts.into_iter().max_by_key(|(_, count)| *count) {
        if count >= 2 {
            return Ok(ip);
        }
    }

    Err(FlareSyncError::IpProvider(
        "Failed to determine public IP by quorum (need 2 of 3 sources to agree)".to_string(),
    ))
}
