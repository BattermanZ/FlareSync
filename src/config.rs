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

        let domain_names: Vec<String> = domain_names_str
            .split(|c| c == ',' || c == ';')
            .map(|s| s.trim().to_string())
            .collect();

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

    fn run_test<T>(test: T)
    where
        T: FnOnce(),
    {
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

        let dotenv_path = std::env::current_dir().unwrap().join(".env");
        let dotenv_backup_path = std::env::current_dir().unwrap().join(".env.backup");
        if dotenv_path.exists() {
            std::fs::rename(&dotenv_path, &dotenv_backup_path).unwrap();
        }

        test();

        for (var, val) in original_vars {
            if let Some(v) = val {
                env::set_var(var, v);
            } else {
                env::remove_var(var);
            }
        }

        if dotenv_backup_path.exists() {
            std::fs::rename(&dotenv_backup_path, &dotenv_path).unwrap();
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
}
