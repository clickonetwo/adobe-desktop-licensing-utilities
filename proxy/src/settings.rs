use config::{ConfigError, Config, File, FileFormat};
use serde::{Deserialize};

#[derive(Clone, Debug, Deserialize)]
pub struct Proxy {
    pub host: String,
    pub remote_host: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Logging {
    pub console_log_level: String,
    pub log_to_file: bool,
    pub file_log_level: String,
    pub file_log_path: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Settings {
    pub proxy: Proxy,
    pub logging: Logging,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let mut s = Config::new();
        s.merge(File::from_str(include_str!("res/defaults.toml"), FileFormat::Toml))?;
        s.try_into()
    }
}
