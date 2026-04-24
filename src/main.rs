use flaresync::cloudflare::{check_and_update_ip, DnsUpdateStatus};
use flaresync::config::Config;
use flaresync::errors::FlareSyncError;
use flaresync::ip_provider::get_current_ip;
use flaresync::status::RuntimeStatus;
use log::{error, info, warn};
use reqwest::Client as ReqwestClient;
use std::net::Ipv4Addr;
use std::time::Duration;
use tokio::time;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log_config_path =
        std::env::var("LOG_CONFIG_PATH").unwrap_or_else(|_| "log4rs.yaml".to_string());
    log4rs::init_file(&log_config_path, Default::default())?;

    let config = Config::from_env()?;

    let client = ReqwestClient::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    info!("FlareSync started");
    let mut status = RuntimeStatus::new();
    write_status(&status, &config);

    loop {
        let current_ip = match wait_for_ip_or_shutdown(&client).await {
            IpCheckOutcome::Shutdown => {
                info!("Shutdown signal received. Exiting.");
                status.mark_shutting_down();
                write_status(&status, &config);
                break;
            }
            IpCheckOutcome::Complete(Ok(ip)) => ip,
            IpCheckOutcome::Complete(Err(e)) => {
                error!("Failed to get current IP: {}. Retrying in 1 minute.", e);
                status.mark_ip_check_error(&e);
                write_status(&status, &config);
                if sleep_or_shutdown(Duration::from_secs(60)).await {
                    info!("Shutdown signal received. Exiting.");
                    status.mark_shutting_down();
                    write_status(&status, &config);
                    break;
                }
                continue;
            }
        };
        info!("Current public IP: {}", current_ip);
        status.mark_ip_check_success(&current_ip);
        write_status(&status, &config);

        let mut shutting_down = false;
        for domain_name in &config.domain_names {
            let update_outcome = tokio::select! {
                result = check_and_update_ip(
                    &client,
                    &config.api_token,
                    &config.zone_id,
                    domain_name,
                    &current_ip,
                ) => DomainUpdateOutcome::Complete(result),
                _ = shutdown_signal() => DomainUpdateOutcome::Shutdown,
            };

            match update_outcome {
                DomainUpdateOutcome::Complete(Ok(update_status)) => {
                    match update_status {
                        DnsUpdateStatus::Updated => {
                            info!("IP address updated successfully for {}", domain_name);
                            status.mark_domain_result(domain_name, "updated", true);
                        }
                        DnsUpdateStatus::Unchanged => {
                            info!("No update needed for {}", domain_name);
                            status.mark_domain_result(domain_name, "unchanged", false);
                        }
                        DnsUpdateStatus::Missing => {
                            info!("No matching DNS record found for {}", domain_name);
                            status.mark_domain_result(domain_name, "missing", false);
                        }
                    }
                    write_status(&status, &config);
                }
                DomainUpdateOutcome::Complete(Err(e)) => {
                    error!("Failed to check or update IP for {}: {}", domain_name, e);
                    status.mark_domain_error(domain_name, &e);
                    write_status(&status, &config);
                }
                DomainUpdateOutcome::Shutdown => {
                    info!("Shutdown signal received. Exiting.");
                    status.mark_shutting_down();
                    write_status(&status, &config);
                    shutting_down = true;
                    break;
                }
            }
        }

        if shutting_down {
            break;
        }

        info!("Waiting for {:?} before next check", config.update_interval);
        if sleep_or_shutdown(config.update_interval).await {
            info!("Shutdown signal received. Exiting.");
            status.mark_shutting_down();
            write_status(&status, &config);
            break;
        }
    }

    Ok(())
}

enum IpCheckOutcome {
    Complete(Result<Ipv4Addr, FlareSyncError>),
    Shutdown,
}

enum DomainUpdateOutcome {
    Complete(Result<DnsUpdateStatus, FlareSyncError>),
    Shutdown,
}

async fn wait_for_ip_or_shutdown(client: &ReqwestClient) -> IpCheckOutcome {
    tokio::select! {
        result = get_current_ip(client) => IpCheckOutcome::Complete(result),
        _ = shutdown_signal() => IpCheckOutcome::Shutdown,
    }
}

async fn sleep_or_shutdown(duration: Duration) -> bool {
    tokio::select! {
        _ = time::sleep(duration) => false,
        _ = shutdown_signal() => true,
    }
}

fn write_status(status: &RuntimeStatus, config: &Config) {
    if let Err(e) = status.write_to_path(&config.status_file_path) {
        warn!(
            "Failed to write status file {}: {}",
            config.status_file_path.display(),
            e
        );
    }
}

#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigterm = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = sigterm.recv() => {}
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
