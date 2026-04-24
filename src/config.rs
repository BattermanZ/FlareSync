use crate::errors::FlareSyncError;
use std::env;
use std::path::PathBuf;
use std::time::Duration;

const DEFAULT_UPDATE_INTERVAL_MINUTES: u64 = 5;
const DEFAULT_STATUS_FILE_PATH: &str = "status/flaresync-status.json";

#[derive(Debug)]
pub struct Config {
    pub api_token: String,
    pub zone_id: String,
    pub domain_names: Vec<String>,
    pub update_interval: Duration,
    pub status_file_path: PathBuf,
}

impl Config {
    pub fn from_env() -> Result<Self, FlareSyncError> {
        dotenvy::dotenv().ok();

        let api_token = env::var("CLOUDFLARE_API_TOKEN")
            .map_err(|_| FlareSyncError::Config("CLOUDFLARE_API_TOKEN must be set".to_string()))?;
        let zone_id = env::var("CLOUDFLARE_ZONE_ID")
            .map_err(|_| FlareSyncError::Config("CLOUDFLARE_ZONE_ID must be set".to_string()))?;
        let domain_names_str = env::var("DOMAIN_NAME")
            .map_err(|_| FlareSyncError::Config("DOMAIN_NAME must be set".to_string()))?;
        let update_interval_minutes: u64 = match env::var("UPDATE_INTERVAL") {
            Ok(value) => value.parse().map_err(|_| {
                FlareSyncError::Config("UPDATE_INTERVAL must be a number".to_string())
            })?,
            Err(_) => DEFAULT_UPDATE_INTERVAL_MINUTES,
        };
        if update_interval_minutes < 1 {
            return Err(FlareSyncError::Config(
                "UPDATE_INTERVAL must be at least 1 minute".to_string(),
            ));
        }
        let update_interval_seconds = update_interval_minutes
            .checked_mul(60)
            .ok_or_else(|| FlareSyncError::Config("UPDATE_INTERVAL is too large".to_string()))?;

        let domain_names: Vec<String> = domain_names_str
            .split([',', ';'])
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        if domain_names.is_empty() {
            return Err(FlareSyncError::Config(
                "DOMAIN_NAME must include at least one non-empty domain".to_string(),
            ));
        }
        let status_file_path = env::var("STATUS_FILE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_STATUS_FILE_PATH));

        Ok(Config {
            api_token,
            zone_id,
            domain_names,
            update_interval: Duration::from_secs(update_interval_seconds),
            status_file_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn run_test<T>(test: T)
    where
        T: FnOnce(),
    {
        let _guard = crate::test_support::global_lock();

        let vars_to_clear = [
            "CLOUDFLARE_API_TOKEN",
            "CLOUDFLARE_ZONE_ID",
            "DOMAIN_NAME",
            "UPDATE_INTERVAL",
            "STATUS_FILE_PATH",
        ];
        let original_vars: Vec<_> = vars_to_clear
            .iter()
            .map(|&var| (var, env::var(var).ok()))
            .collect();

        for &var in &vars_to_clear {
            env::remove_var(var);
        }

        let original_cwd = std::env::current_dir().unwrap();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let test_cwd = std::env::temp_dir().join(format!(
            "flaresync_test_env_{}_{}",
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(&test_cwd).unwrap();
        std::env::set_current_dir(&test_cwd).unwrap();

        test();

        std::env::set_current_dir(&original_cwd).unwrap();
        std::fs::remove_dir_all(&test_cwd).ok();

        for (var, val) in original_vars {
            if let Some(v) = val {
                env::set_var(var, v);
            } else {
                env::remove_var(var);
            }
        }
    }

    #[test]
    fn test_config_from_env_missing_vars() {
        run_test(|| {
            let result = Config::from_env();
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_config_from_env_success() {
        run_test(|| {
            env::set_var("CLOUDFLARE_API_TOKEN", "test_token");
            env::set_var("CLOUDFLARE_ZONE_ID", "test_zone_id");
            env::set_var("DOMAIN_NAME", "example.com;another.com");
            env::set_var("UPDATE_INTERVAL", "15");

            let config = Config::from_env().unwrap();
            assert_eq!(config.api_token, "test_token");
            assert_eq!(config.zone_id, "test_zone_id");
            assert_eq!(config.domain_names, vec!["example.com", "another.com"]);
            assert_eq!(config.update_interval, Duration::from_secs(15 * 60));
            assert_eq!(
                config.status_file_path,
                PathBuf::from(DEFAULT_STATUS_FILE_PATH)
            );
        });
    }

    #[test]
    fn test_config_from_env_defaults_update_interval() {
        run_test(|| {
            env::set_var("CLOUDFLARE_API_TOKEN", "test_token");
            env::set_var("CLOUDFLARE_ZONE_ID", "test_zone_id");
            env::set_var("DOMAIN_NAME", "example.com");

            let config = Config::from_env().unwrap();
            assert_eq!(
                config.update_interval,
                Duration::from_secs(DEFAULT_UPDATE_INTERVAL_MINUTES * 60)
            );
        });
    }

    #[test]
    fn test_config_from_env_accepts_custom_status_file_path() {
        run_test(|| {
            env::set_var("CLOUDFLARE_API_TOKEN", "test_token");
            env::set_var("CLOUDFLARE_ZONE_ID", "test_zone_id");
            env::set_var("DOMAIN_NAME", "example.com");
            env::set_var("STATUS_FILE_PATH", "/tmp/flaresync-status.json");

            let config = Config::from_env().unwrap();
            assert_eq!(
                config.status_file_path,
                PathBuf::from("/tmp/flaresync-status.json")
            );
        });
    }

    #[test]
    fn test_config_from_env_filters_empty_domains() {
        run_test(|| {
            env::set_var("CLOUDFLARE_API_TOKEN", "test_token");
            env::set_var("CLOUDFLARE_ZONE_ID", "test_zone_id");
            env::set_var("DOMAIN_NAME", "example.com, ,;another.com,,");
            env::set_var("UPDATE_INTERVAL", "15");

            let config = Config::from_env().unwrap();
            assert_eq!(config.domain_names, vec!["example.com", "another.com"]);
        });
    }

    #[test]
    fn test_config_from_env_rejects_zero_interval() {
        run_test(|| {
            env::set_var("CLOUDFLARE_API_TOKEN", "test_token");
            env::set_var("CLOUDFLARE_ZONE_ID", "test_zone_id");
            env::set_var("DOMAIN_NAME", "example.com");
            env::set_var("UPDATE_INTERVAL", "0");

            let result = Config::from_env();
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_config_from_env_rejects_interval_overflow() {
        run_test(|| {
            env::set_var("CLOUDFLARE_API_TOKEN", "test_token");
            env::set_var("CLOUDFLARE_ZONE_ID", "test_zone_id");
            env::set_var("DOMAIN_NAME", "example.com");
            env::set_var("UPDATE_INTERVAL", u64::MAX.to_string());

            let result = Config::from_env();
            assert!(matches!(result, Err(FlareSyncError::Config(_))));
        });
    }
}
