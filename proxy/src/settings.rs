use config::{ConfigError, Config, File as ConfigFile, FileFormat};
use serde::{Deserialize};
use eyre::{eyre, Result};
use std::fs::File;
use std::io::prelude::*;

#[derive(Clone, Debug, Deserialize)]
pub struct Proxy {
    pub host: String,
    pub remote_host: String,
    pub ssl: Option<bool>,
    pub ssl_cert: Option<String>,
    pub ssl_key: Option<String>,
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
    pub fn new(config_file: Option<String>, host: Option<String>, remote_host: Option<String>,
        ssl: Option<bool>, ssl_cert: Option<String>, ssl_key: Option<String>) -> Result<Self, ConfigError> {

        let mut s = Config::new();
        s.merge(ConfigFile::from_str(include_str!("res/defaults.toml"), FileFormat::Toml))?;
        if let Some(filename) = config_file {
            s.merge(ConfigFile::with_name(filename.as_str()))?;
        }
        if let Some(host) = host {
            s.set("proxy.host", host)?;
        }
        if let Some(remote_host) = remote_host {
            s.set("proxy.remote_host", remote_host)?;
        }
        if let Some(ssl) = ssl {
            s.set("proxy.ssl", ssl)?;
        }
        if let Some(ssl_cert) = ssl_cert {
            s.set("proxy.ssl_cert", ssl_cert)?;
        }
        if let Some(ssl_key) = ssl_key {
            s.set("proxy.ssl_key", ssl_key)?;
        }
        s.try_into()
    }

    pub fn validate(self: &Self) -> Result<()> {
        match self.proxy.ssl {
            Some(ssl_enable) => {
                if ssl_enable && (self.proxy.ssl_cert.is_none() || self.proxy.ssl_key.is_none()) {
                    Err(eyre!("ssl_cert and ssl_key must be specified if SSL is enabled"))
                } else {
                    Ok(())
                }
            }
            None => Ok(()) 
        }
    }
}

pub fn config_template(filename: String) -> std::io::Result<()> {
    let template = include_str!("res/template.toml");
    let mut file = File::create(filename)?;
    file.write_all(template.as_bytes())?;
    Ok(())
}
