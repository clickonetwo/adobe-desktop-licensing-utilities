/*
Copyright 2020 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
use crate::cli::FrlProxy;
use config::{Config, Environment, File as ConfigFile, FileFormat};
use eyre::{eyre, Report, Result, WrapErr};
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use std::fs::File;
use std::io::prelude::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proxy {
    pub mode: ProxyMode,
    pub host: String,
    pub remote_host: String,
    pub ssl: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ssl {
    pub cert_path: String,
    pub cert_password: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Logging {
    pub level: LogLevel,
    pub destination: LogDestination,
    pub file_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cache {
    pub db_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub proxy: Proxy,
    pub ssl: Ssl,
    pub logging: Logging,
    pub cache: Cache,
}

impl Settings {
    pub fn load_config(args: &FrlProxy) -> Result<Option<Self>> {
        let path = args.config_file.as_str();
        if let Ok(_) = std::fs::metadata(path) {
            let mut s = Config::new();
            s.merge(ConfigFile::from_str(
                include_str!("res/defaults.toml"),
                FileFormat::Toml,
            ))?;
            s.merge(ConfigFile::with_name(path).format(FileFormat::Toml))?;
            s.merge(Environment::with_prefix("frl_proxy"))?;
            let mut conf: Self = s.try_into()?;
            match args.debug {
                1 => conf.logging.level = LogLevel::Debug,
                2 => conf.logging.level = LogLevel::Trace,
                _ => {}
            }
            if let Some(log_to) = &args.log_to {
                let destination: LogDestination = log_to
                    .as_str()
                    .try_into()
                    .wrap_err(format!("Not a recognized log destination: {}", log_to))?;
                conf.logging.destination = destination;
            }
            Ok(Some(conf))
        } else {
            eprintln!("Creating initial configuration file...");
            let template = include_str!("res/defaults.toml");
            let mut file = File::create(path)
                .wrap_err(format!("Cannot create config file: {}", path))?;
            file.write_all(template.as_bytes())
                .wrap_err(format!("Cannot write config file: {}", path))?;
            Ok(None)
        }
    }

    pub fn update_config(&self, path: &str) -> Result<()> {
        // update proxy settings
        // update ssl settings
        // update cache settings
        // update log settings
        let toml = toml::to_string(self)
            .wrap_err(format!("Cannot serialize configuration: {:?}", self))?;
        let mut file = File::create(path)
            .wrap_err(format!("Cannot create config file: {}", path))?;
        file.write_all(toml.as_bytes())
            .wrap_err(format!("Cannot write config file: {}", path))?;
        eprintln!("Wrote config file '{}'", path);
        Ok(())
    }

    pub fn validate(&mut self) -> Result<()> {
        if self.proxy.ssl {
            let path = &self.ssl.cert_path;
            if path.is_empty() {
                return Err(eyre!("Certificate path can't be empty when SSL is enabled"));
            }
            std::fs::metadata(path)
                .wrap_err(format!("Invalid certificate path: {}", path))?;
        }
        if let ProxyMode::Cache | ProxyMode::Store | ProxyMode::Forward = self.proxy.mode
        {
            if self.cache.db_path.is_empty() {
                return Err(eyre!("Database path can't be empty when cache is enabled"));
            }
        }
        if let LogDestination::File = self.logging.destination {
            if self.logging.file_path.is_empty() {
                return Err(eyre!("File path must be specified when logging to a file"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    Cache,
    Store,
    Forward,
    Passthrough,
}

impl Default for ProxyMode {
    fn default() -> Self {
        ProxyMode::Cache
    }
}

impl TryFrom<&str> for ProxyMode {
    type Error = Report;

    fn try_from(s: &str) -> Result<Self> {
        let sl = s.to_ascii_lowercase();
        if "cache".starts_with(&sl) {
            Ok(ProxyMode::Cache)
        } else if "store".starts_with(&sl) {
            Ok(ProxyMode::Store)
        } else if "forward".starts_with(&sl) {
            Ok(ProxyMode::Store)
        } else if "passthrough".starts_with(&sl) {
            Ok(ProxyMode::Store)
        } else {
            Err(eyre!("proxy mode '{}' must be a prefix of cache, store, forward or passthrough", s))
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogDestination {
    #[serde(alias = "c")]
    Console,
    #[serde(alias = "f")]
    File,
}

impl Default for LogDestination {
    fn default() -> Self {
        LogDestination::Console
    }
}

impl TryFrom<&str> for LogDestination {
    type Error = Report;

    fn try_from(s: &str) -> Result<Self> {
        let sl = s.to_ascii_lowercase();
        if "console".starts_with(&sl) {
            Ok(LogDestination::Console)
        } else if "file".starts_with(&sl) {
            Ok(LogDestination::File)
        } else {
            Err(eyre!("log destination '{}' must be a prefix of console or file", s))
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Default for LogLevel {
    fn default() -> Self {
        LogLevel::Info
    }
}

impl TryFrom<&str> for LogLevel {
    type Error = Report;

    fn try_from(s: &str) -> Result<Self> {
        let sl = s.to_ascii_lowercase();
        if "off".starts_with(&sl) {
            Ok(LogLevel::Off)
        } else if "error".starts_with(&sl) {
            Ok(LogLevel::Error)
        } else if "warn".starts_with(&sl) {
            Ok(LogLevel::Warn)
        } else if "info".starts_with(&sl) {
            Ok(LogLevel::Info)
        } else if "debug".starts_with(&sl) {
            Ok(LogLevel::Debug)
        } else if "trace".starts_with(&sl) {
            Ok(LogLevel::Trace)
        } else {
            Err(eyre!("log level '{}' must be a prefix of off, error, warn, info, debug, or trace", s))
        }
    }
}
