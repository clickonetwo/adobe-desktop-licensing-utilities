/*
Copyright 2020 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
use crate::cli::FrlProxy;
use config::{Config, Environment, File as ConfigFile, FileFormat};
use dialoguer::{Confirm, Input, Password, Select};
use eyre::{eyre, Report, Result, WrapErr};
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use std::fs::File;
use std::io::prelude::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proxy {
    pub mode: ProxyMode,
    pub host: String,
    pub port: String,
    pub ssl_port: String,
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
        if std::fs::metadata(path).is_ok() {
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

    pub fn update_config_file(&mut self, path: &str) -> Result<()> {
        // update proxy settings including cache db
        eprintln!("The proxy has four modes: cache, store, forward, and passthrough.");
        eprintln!("Read the user guide to understand which is right for each situation.");
        let choices = vec!["cache", "store", "forward", "passthrough"];
        let default = match self.proxy.mode {
            ProxyMode::Cache => 0,
            ProxyMode::Store => 1,
            ProxyMode::Forward => 2,
            ProxyMode::Passthrough => 3,
        };
        let choice = Select::new()
            .items(&choices)
            .default(default)
            .with_prompt("Proxy mode")
            .interact()?;
        let choice: ProxyMode = choices[choice].try_into().unwrap();
        self.proxy.mode = choice;
        if let ProxyMode::Cache | ProxyMode::Store | ProxyMode::Forward = self.proxy.mode
        {
            eprintln!("The proxy uses a SQLite database to keep track of requests and responses.");
            eprintln!(
                "The proxy will create this database if one does not already exist."
            );
            let choice: String = Input::new()
                .allow_empty(false)
                .with_prompt("Name of (or path to) your database file")
                .with_initial_text(&self.cache.db_path)
                .interact_text()?;
            self.cache.db_path = choice;
        }
        eprintln!(
            "The host and port of the proxy must match the one in your license package."
        );
        let choice: String = Input::new()
            .with_prompt("Host IP to listen on")
            .with_initial_text(&self.proxy.host)
            .interact_text()?;
        self.proxy.host = choice;
        let choice: String = Input::new()
            .with_prompt("Host port for http (non-ssl) mode")
            .with_initial_text(&self.proxy.port)
            .interact_text()?;
        self.proxy.port = choice;
        eprintln!("Your proxy server must contact one of two Adobe licensing servers.");
        eprintln!("Use the variable IP server unless your firewall doesn't permit it.");
        let choices = vec![
            "Variable IP server (lcs-cops.adobe.io)",
            "Fixed IP server (lcs-cops-proxy.adobe.com)",
        ];
        let default = if self.proxy.remote_host == "https://lcs-cops-proxy.adobe.com" {
            1
        } else {
            0
        };
        let choice = Select::new()
            .items(&choices)
            .default(default)
            .with_prompt("Adobe licensing server")
            .interact()?;
        self.proxy.remote_host = if choice == 0usize {
            String::from("https://lcs-cops.adobe.io")
        } else {
            String::from("https://lcs-cops-proxy.adobe.com")
        };
        eprintln!("MacOS applications can only connect to the proxy via SSL.");
        eprintln!("Windows applications can use SSL, but they don't require it.");
        let choice = Confirm::new()
            .default(self.proxy.ssl)
            .show_default(true)
            .wait_for_newline(false)
            .with_prompt("Use SSL?")
            .interact()?;
        self.proxy.ssl = choice;
        // update ssl settings
        if self.proxy.ssl {
            let choice: String = Input::new()
                .with_prompt("Host port for https mode")
                .with_initial_text(&self.proxy.ssl_port)
                .interact_text()?;
            self.proxy.ssl_port = choice;
            eprintln!(
                "The proxy requires a certificate store in PKCS format to use SSL."
            );
            eprintln!(
                "Read the user guide to learn how to obtain and prepare this file."
            );
            let mut need_cert = true;
            let mut choice = self.ssl.cert_path.clone();
            while need_cert {
                choice = Input::new()
                    .with_prompt("Name of (or path to) your cert file")
                    .with_initial_text(choice)
                    .interact_text()?;
                if std::fs::metadata(&choice).is_ok() {
                    self.ssl.cert_path = choice.clone();
                    need_cert = false;
                } else {
                    eprintln!("There is no certificate at that path, try again.");
                }
            }
            eprintln!("Usually, for security, PKCS files are encrypted with a password.");
            eprintln!(
                "Your proxy will require that password in order to function properly."
            );
            eprintln!(
                "You have the choice of storing your password in your config file or"
            );
            eprintln!(
                "in the value of an environment variable (FRL_PROXY_SSL.CERT_PASSWORD)."
            );
            let prompt = if self.ssl.cert_password.is_empty() {
                "Do you want to store a password in your configuration file?"
            } else {
                "Do you want to update the password in your configuration file?"
            };
            let choice = Confirm::new()
                .default(false)
                .wait_for_newline(false)
                .with_prompt(prompt)
                .interact()?;
            if choice {
                let choice = Password::new()
                    .with_prompt("Enter password")
                    .with_confirmation("Confirm password", "Passwords don't match")
                    .allow_empty_password(true)
                    .interact()?;
                self.ssl.cert_password = choice;
            }
        }
        // update log settings
        let prompt = if let LogLevel::Off = self.logging.level {
            "Do you want your proxy server to log information about its operation?"
        } else {
            "Do you want to customize your proxy server's logging configuration?"
        };
        let choice = Confirm::new()
            .default(false)
            .wait_for_newline(false)
            .with_prompt(prompt)
            .interact()?;
        if choice {
            eprintln!("The proxy can log to the console (standard output) or to a file on disk.");
            let choices = vec!["console", "disk file"];
            let choice = Select::new()
                .items(&choices)
                .default(1)
                .with_prompt("Log destination")
                .interact()?;
            self.logging.destination =
                if choice == 0 { LogDestination::Console } else { LogDestination::File };
            if choice == 1 {
                let choice: String = Input::new()
                    .allow_empty(false)
                    .with_prompt("Name of (or path to) your log file")
                    .with_initial_text(&self.logging.file_path)
                    .interact_text()?;
                self.logging.file_path = choice;
            }
            let mut choice = !matches!(self.logging.level, LogLevel::Info);
            if !choice {
                eprintln!("The proxy will log errors, warnings and summary information.");
                choice = Confirm::new()
                    .default(false)
                    .wait_for_newline(false)
                    .with_prompt("Do you want to adjust the level of logged information?")
                    .interact()?;
            }
            if choice {
                eprintln!("Read the user guide to find out more about logging levels.");
                let choices =
                    vec!["no logging", "error", "warn", "info", "debug", "trace"];
                let default = match self.logging.level {
                    LogLevel::Off => 0,
                    LogLevel::Error => 1,
                    LogLevel::Warn => 2,
                    LogLevel::Info => 3,
                    LogLevel::Debug => 4,
                    LogLevel::Trace => 5,
                };
                let choice = Select::new()
                    .items(&choices)
                    .default(default)
                    .with_prompt("Log level")
                    .interact()?;
                let choice: LogLevel = choices[choice].try_into().unwrap();
                self.logging.level = choice;
            }
        } else {
            self.logging.level = LogLevel::Off;
            self.logging.destination = LogDestination::Console;
        }
        // save the configuration
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
        if self.proxy.host.contains(":") {
            return Err(eyre!("Host must not contain a port (use the 'port' and 'ssl_port' config options)"));
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
            Ok(ProxyMode::Forward)
        } else if "passthrough".starts_with(&sl) {
            Ok(ProxyMode::Passthrough)
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
