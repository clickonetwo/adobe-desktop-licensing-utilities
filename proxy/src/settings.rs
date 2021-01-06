/*
Copyright 2020 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
use config::{Config, ConfigError, File as ConfigFile, FileFormat};
use eyre::{eyre, Result};
use serde::Deserialize;
use std::fs::File;
use std::io::prelude::*;

#[derive(Default, Clone, Debug, Deserialize)]
pub struct Proxy {
    pub mode: String,
    pub host: String,
    pub remote_host: String,
    pub ssl: Option<bool>,
    pub ssl_cert: Option<String>,
    pub ssl_password: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Logging {
    pub console_log_level: String,
    pub log_to_file: bool,
    pub file_log_level: String,
    pub file_log_path: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Cache {
    pub cache_file_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Settings {
    pub proxy: Proxy,
    pub logging: Logging,
    pub cache: Cache,
}

impl Settings {
    pub fn from_start(
        config_file: Option<String>, mode: Option<String>, host: Option<String>,
        remote_host: Option<String>, ssl: Option<bool>, ssl_cert: Option<String>,
        ssl_key: Option<String>,
    ) -> Result<Self, ConfigError> {
        let mut s = Config::new();
        s.merge(ConfigFile::from_str(
            include_str!("res/defaults.toml"),
            FileFormat::Toml,
        ))?;
        if let Some(filename) = config_file {
            s.merge(ConfigFile::with_name(filename.as_str()))?;
        }
        if let Some(mode) = mode {
            s.set("proxy.mode", mode)?;
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

    pub fn from_cache_control(config_file: Option<String>) -> Result<Self, ConfigError> {
        let mut s = Config::new();
        s.merge(ConfigFile::from_str(
            include_str!("res/defaults.toml"),
            FileFormat::Toml,
        ))?;
        if let Some(filename) = config_file {
            s.merge(ConfigFile::with_name(filename.as_str()))?;
        }
        // force enablement of the cache if doing cache control
        s.set("proxy.mode", "cache")?;
        s.try_into()
    }

    pub fn validate(&mut self) -> Result<()> {
        self.proxy.mode = self.proxy.mode.to_ascii_lowercase();
        let mode = self.proxy.mode.as_str();
        if !"cache".starts_with(mode) && !"passthrough".starts_with(mode) && !"store".starts_with(mode) && !"forward".starts_with(mode) {
            return Err(eyre!("Mode must be cache, passthrough, store, or forward"));
        }
        if let Some(true) = self.proxy.ssl {
            if self.proxy.ssl_cert.is_none() || self.proxy.ssl_password.is_none() {
                return Err(eyre!(
                    "ssl_cert and ssl_key must be specified if SSL is enabled"
                ));
            } else if self.proxy.ssl_cert.as_ref().unwrap().is_empty() {
                return Err(eyre!("ssl_cert pathname cannot be an empty string"));
            } else if self.proxy.ssl_password.as_ref().unwrap().is_empty() {
                return Err(eyre!("ssl_key pathname cannot be an empty string"));
            }
        }
        if self.proxy.mode.starts_with('p') {
            // don't need a cache file, so fall through to next check
        } else if self.cache.cache_file_path.is_none() {
            return Err(eyre!(
                "cache_file_path must be specified if the cache is enabled"
            ));
        } else if self.cache.cache_file_path.as_ref().unwrap().is_empty() {
            return Err(eyre!("The cache_file_path cannot be an empty string"));
        }
        Ok(())
    }
}

pub fn config_template(filename: String) -> std::io::Result<()> {
    let template = include_str!("res/template.toml");
    let mut file = File::create(&filename)?;
    file.write_all(template.as_bytes())?;
    println!("Created config file '{}'", filename);
    Ok(())
}
