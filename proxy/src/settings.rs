use config::{ConfigError, Config, File as ConfigFile, FileFormat};
use serde::{Deserialize};
use std::fs::File;
use std::io::prelude::*;

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
    pub fn new(config_file: Option<String>) -> Result<Self, ConfigError> {
        let mut s = Config::new();
        s.merge(ConfigFile::from_str(include_str!("res/defaults.toml"), FileFormat::Toml))?;
        match config_file {
            Some(filename) => { s.merge(ConfigFile::with_name(filename.as_str()))?; }
            None => ()
        }
        s.try_into()
    }
}

pub fn config_template(filename: String) -> std::io::Result<()> {
    let template = include_str!("res/template.toml");
    let mut file = File::create(filename)?;
    file.write_all(template.as_bytes())?;
    Ok(())
}
