use std::path::PathBuf;

pub const RATSPEAK_RNS_CONFIG_DIR_ENV: &str = "RATSPEAK_RNS_CONFIG_DIR";
pub const RATSPEAK_RNS_SHARED_INSTANCE_PORT: u16 = 37_430;
pub const RATSPEAK_RNS_INSTANCE_CONTROL_PORT: u16 = 37_431;
pub const LEGACY_RNS_SHARED_INSTANCE_PORT: u16 = 37_428;
pub const LEGACY_RNS_INSTANCE_CONTROL_PORT: u16 = 37_429;

#[derive(Debug, Clone)]
pub struct DashboardConfig {
    pub data_root: PathBuf,
    pub data_dir: PathBuf,
    pub rns_config_dir: PathBuf,
    pub rns_config_dir_overridden: bool,
    pub port: u16,
    pub api_token: String,
    pub poll_interval: f64,
    pub max_log_entries: usize,
}

impl DashboardConfig {
    pub fn from_env_and_defaults(data_root: PathBuf) -> Self {
        let data_dir = data_root.join(".ratspeak");
        std::fs::create_dir_all(&data_dir).ok();

        let rns_config_dir_override = std::env::var_os(RATSPEAK_RNS_CONFIG_DIR_ENV);
        let rns_config_dir_overridden = rns_config_dir_override.is_some();
        let rns_config_dir = rns_config_dir_override
            .map(PathBuf::from)
            .unwrap_or_else(|| data_dir.join("reticulum"));

        Self {
            data_root,
            data_dir,
            rns_config_dir,
            rns_config_dir_overridden,
            port: std::env::var("RATSPEAK_SERVER_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5050),
            api_token: std::env::var("RATSPEAK_SERVER_API_TOKEN").unwrap_or_default(),
            poll_interval: 1.5,
            max_log_entries: 200,
        }
    }

    pub fn uses_app_private_rns_config_dir(&self) -> bool {
        !self.rns_config_dir_overridden
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("ratspeak.db")
    }

    pub fn files_dir(&self) -> PathBuf {
        let d = self.data_dir.join("files");
        std::fs::create_dir_all(&d).ok();
        d
    }

    pub fn identities_dir(&self) -> PathBuf {
        let d = self.data_dir.join("identities");
        std::fs::create_dir_all(&d).ok();
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rns_config_dir_lives_under_ratspeak_data_dir() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let data_root = std::env::temp_dir().join(format!(
            "ratspeak-config-test-{}-{nanos}",
            std::process::id()
        ));
        let config = DashboardConfig::from_env_and_defaults(data_root.clone());

        assert_eq!(config.data_dir, data_root.join(".ratspeak"));
        assert_eq!(config.rns_config_dir, data_root.join(".ratspeak/reticulum"));
        assert!(config.uses_app_private_rns_config_dir());
    }
}
