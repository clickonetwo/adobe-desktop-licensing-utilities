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
        ssl: bool, ssl_cert: Option<String>, ssl_key: Option<String>) -> Result<Self, ConfigError> {

        let mut s = Config::new();
        s.merge(ConfigFile::from_str(include_str!("res/defaults.toml"), FileFormat::Toml))?;
        match config_file {
            Some(filename) => { s.merge(ConfigFile::with_name(filename.as_str()))?; }
            None => ()
        }
        match host {
            Some(host) => { s.set("proxy.host", host)?; }
            None => ()
        }
        match remote_host {
            Some(remote_host) => { s.set("proxy.remote_host", remote_host)?; }
            None => ()
        }
        s.set("proxy.ssl", ssl)?;
        match ssl_cert {
            Some(ssl_cert) => { s.set("proxy.ssl_cert", ssl_cert)?; }
            None => ()
        }
        match ssl_key {
            Some(ssl_key) => { s.set("proxy.ssl_key", ssl_key)?; }
            None => ()
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
