use crate::errors::FlareSyncError;
use std::env;
use std::time::Duration;

#[derive(Debug)]
pub struct Config {
    pub api_token: String,
    pub zone_id: String,
    pub domain_names: Vec<String>,
    pub update_interval: Duration,
}

impl Config {
    pub fn from_env() -> Result<Self, FlareSyncError> {
        dotenv::dotenv().ok();

        let api_token = env::var("CLOUDFLARE_API_TOKEN")
            .map_err(|_| FlareSyncError::Config("CLOUDFLARE_API_TOKEN must be set".to_string()))?;
        let zone_id = env::var("CLOUDFLARE_ZONE_ID")
            .map_err(|_| FlareSyncError::Config("CLOUDFLARE_ZONE_ID must be set".to_string()))?;
        let domain_names_str = env::var("DOMAIN_NAME")
            .map_err(|_| FlareSyncError::Config("DOMAIN_NAME must be set".to_string()))?;
        let update_interval_minutes: u64 = env::var("UPDATE_INTERVAL")
            .map_err(|_| FlareSyncError::Config("UPDATE_INTERVAL must be set".to_string()))?
            .parse()
            .map_err(|_| FlareSyncError::Config("UPDATE_INTERVAL must be a number".to_string()))?;
        if update_interval_minutes < 1 {
            return Err(FlareSyncError::Config(
                "UPDATE_INTERVAL must be at least 1 minute".to_string(),
            ));
        }

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

        Ok(Config {
            api_token,
            zone_id,
            domain_names,
            update_interval: Duration::from_secs(update_interval_minutes * 60),
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
}
