/*
Copyright 2022 Daniel Brotsky. All rights reserved.

All of the copyrighted work in this repository is licensed under the
GNU Affero General Public License, reproduced in the LICENSE-AGPL file.

Attribution:

Some source files in this repository are derived from files in two Adobe Open
Source projects: the Adobe License Decoder repository found at this URL:
    https://github.com/adobe/adobe-license-decoder.rs
and the FRL Online Proxy repository found at this URL:
    https://github.com/adobe/frl-online-proxy

The files in those original works are copyright 2022 Adobe and the use of those
materials in this work is permitted by the MIT license under which they were
released.  That license is reproduced here in the LICENSE-MIT file.
*/
use std::convert::{TryFrom, TryInto};
use std::fmt::{Debug, Formatter};
use std::fs::File;
use std::io::prelude::*;
use std::sync::Arc;

use config::{Config, Environment, File as ConfigFile, FileFormat};
use dialoguer::{Confirm, Input, Password, Select};
use eyre::{eyre, Report, Result, WrapErr};
use serde::{Deserialize, Serialize};

use crate::cli::FrlProxy;
use crate::Command;

#[derive(Debug, Clone)]
pub struct ProxyConfiguration {
    pub settings: Settings,
    pub cache: crate::Cache,
    pub client: reqwest::Client,
    pub bind_addr: std::net::SocketAddr,
    pub adobe_server: String,
}

impl ProxyConfiguration {
    pub fn new(settings: &Settings, cache: &crate::Cache) -> Self {
        let bad_config = "Proxy Configuration failure (please report a bug)";
        let cops_uri: http::Uri = settings.proxy.remote_host.parse().expect(bad_config);
        let cops_host = cops_uri.host().expect(bad_config);
        let mut builder = reqwest::Client::builder();
        builder = builder.timeout(std::time::Duration::new(59, 0));
        if settings.network.use_proxy {
            let proxy_host = format!(
                "{}://{}:{}",
                "http", settings.network.proxy_host, settings.network.proxy_port
            );
            let mut proxy = reqwest::Proxy::https(&proxy_host).expect(bad_config);
            if settings.network.use_basic_auth {
                proxy = proxy.basic_auth(
                    &settings.network.proxy_username,
                    &settings.network.proxy_password,
                );
            }
            builder = builder.proxy(proxy)
        }
        let addr = if settings.proxy.ssl {
            format!("{}:{}", settings.proxy.host, settings.proxy.port)
        } else {
            format!("{}:{}", settings.proxy.host, settings.proxy.ssl_port)
        };
        let bind_addr: std::net::SocketAddr =
            addr.parse().expect("Invalid proxy bind address (please report a bug)");
        ProxyConfiguration {
            settings: settings.clone(),
            cache: cache.clone(),
            client: builder.build().expect(bad_config),
            bind_addr,
            adobe_server: if let Some(port) = cops_uri.port() {
                format!("https://{}:{}", cops_host, port.as_str())
            } else {
                format!("https://{}", cops_host)
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proxy {
    pub mode: ProxyMode,
    pub host: String,
    pub port: String,
    pub ssl_port: String,
    pub remote_host: String,
    pub ssl: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Ssl {
    pub cert_path: String,
    pub cert_password: String,
}

impl Debug for Ssl {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ssl")
            .field("cert_path", &self.cert_path)
            .field("password", &String::from("[OBSCURED]"))
            .finish()
    }
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

#[derive(Clone, Serialize, Deserialize)]
pub struct Network {
    pub use_proxy: bool,
    pub proxy_host: String,
    pub proxy_port: String,
    pub use_basic_auth: bool,
    pub proxy_username: String,
    pub proxy_password: String,
}

impl Debug for Network {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Network")
            .field("use_proxy", &self.use_proxy)
            .field("proxy_host", &self.proxy_host)
            .field("proxy_port", &self.proxy_port)
            .field("use_proxy", &self.use_proxy)
            .field("proxy_username", &self.proxy_username)
            .field("proxy_password", &String::from("[OBSCURED]"))
            .finish()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingsRef {
    pub proxy: Proxy,
    pub ssl: Ssl,
    pub logging: Logging,
    pub cache: Cache,
    pub network: Network,
}

pub type Settings = Arc<SettingsRef>;

/// Load settings from the configuration file
pub fn load_config_file(args: &FrlProxy) -> Result<Settings> {
    Ok(Settings::new(SettingsRef::load_config(args)?))
}

/// Update (or create) a configuration file after interviewing user
/// No logging on this path, because it might interfere with the interview
pub fn update_config_file(settings: Option<&Settings>, path: &str) -> Result<()> {
    // get the configuration
    let mut conf: SettingsRef = match settings {
        Some(settings) => settings.as_ref().clone(),
        None => SettingsRef::default_config(),
    };
    // interview the user for updates
    conf.update_config().wrap_err("Configuration interview failed")?;
    // save the configuration
    let toml = toml::to_string(&conf)
        .wrap_err(format!("Cannot serialize configuration: {:?}", &conf))?;
    let mut file =
        File::create(path).wrap_err(format!("Cannot create config file: {}", path))?;
    file.write_all(toml.as_bytes())
        .wrap_err(format!("Cannot write config file: {}", path))?;
    eprintln!("Wrote config file '{}'", path);
    Ok(())
}

impl SettingsRef {
    /// Create a new default config
    pub fn default_config() -> Self {
        let base_str = include_str!("res/defaults.toml");
        let builder = Config::builder()
            .add_source(ConfigFile::from_str(base_str, FileFormat::Toml));
        let conf: Self = builder
            .build()
            .expect("Can't build default configuration (please report a bug)")
            .try_deserialize()
            .expect("Can't create default configuration (please report a bug");
        conf
    }

    /// Load an existing config file, returning its contained config
    pub fn load_config(args: &FrlProxy) -> Result<Self> {
        let base_str = include_str!("res/defaults.toml");
        let builder = Config::builder()
            .add_source(ConfigFile::from_str(base_str, FileFormat::Toml))
            .add_source(ConfigFile::new(&args.config_file, FileFormat::Toml))
            .add_source(Environment::with_prefix("frl_proxy"));
        let mut settings: Self = builder.build()?.try_deserialize()?;
        // Now process the args as overrides: global first, then command-specific
        match args.debug {
            1 => settings.logging.level = LogLevel::Debug,
            2 => settings.logging.level = LogLevel::Trace,
            _ => {}
        }
        if let Some(log_to) = &args.log_to {
            let destination: LogDestination = log_to
                .as_str()
                .try_into()
                .wrap_err(format!("Not a recognized log destination: {}", log_to))?;
            settings.logging.destination = destination;
        }
        match &args.cmd {
            Command::Start { mode, ssl } => {
                if let Some(mode) = mode {
                    settings.proxy.mode = mode.as_str().try_into()?;
                }
                if let Some(ssl) = ssl {
                    settings.proxy.ssl = *ssl;
                }
            }
            Command::Clear { .. } | Command::Import { .. } | Command::Export { .. } => {
                settings.proxy.mode = ProxyMode::Cache;
                // log to file, because this command is interactive
                settings.logging.destination = LogDestination::File;
            }
            Command::Configure => {
                // no logging, because it might interfere with interviews
                settings.logging.level = LogLevel::Off;
            }
        }
        settings.validate()?;
        Ok(settings)
    }

    /// Update configuration settings by interviewing user
    /// No logging on this path, because it might interfere with the interview
    pub fn update_config(&mut self) -> Result<()> {
        // update configuration file by interviewing user
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
        // update network settings
        let prompt = "Does your network require this proxy to use an upstream proxy?";
        let choice = Confirm::new()
            .default(self.network.use_proxy)
            .wait_for_newline(false)
            .with_prompt(prompt)
            .interact()?;
        self.network.use_proxy = choice;
        if choice {
            let choice: String = Input::new()
                .with_prompt("Proxy host")
                .with_initial_text(&self.network.proxy_host)
                .interact_text()?;
            self.network.proxy_host = choice;
            let choice: String = Input::new()
                .with_prompt("Proxy port")
                .with_initial_text(&self.network.proxy_port)
                .interact_text()?;
            self.network.proxy_port = choice;
            let prompt = "Does your upstream proxy require (basic) authentication?";
            let choice = Confirm::new()
                .default(self.network.use_basic_auth)
                .wait_for_newline(false)
                .with_prompt(prompt)
                .interact()?;
            self.network.use_basic_auth = choice;
            if choice {
                let choice: String = Input::new()
                    .with_prompt("Proxy username")
                    .with_initial_text(&self.network.proxy_username)
                    .interact_text()?;
                self.network.proxy_username = choice;
                let choice: String = Input::new()
                    .with_prompt("Proxy password")
                    .with_initial_text(&self.network.proxy_password)
                    .interact_text()?;
                self.network.proxy_password = choice;
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
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        let bind_addr = if self.proxy.ssl {
            format!("{}:{}", self.proxy.host, self.proxy.port)
        } else {
            format!("{}:{}", self.proxy.host, self.proxy.ssl_port)
        };
        if bind_addr.parse::<std::net::SocketAddr>().is_err() {
            return Err(eyre!(
                "Host must be a dotted IP address (e.g., 0.0.0.0); port must be numeric (e.g., 8080)"
            ));
        }
        if self.proxy.ssl {
            let path = &self.ssl.cert_path;
            if path.is_empty() {
                return Err(eyre!("Certificate path can't be empty when SSL is enabled"));
            }
            std::fs::metadata(path)
                .wrap_err(format!("Invalid certificate path: {}", path))?;
        }
        let cops: http::Uri =
            self.proxy.remote_host.parse().wrap_err("Invalid Adobe endpoint")?;
        if cops.scheme_str().unwrap_or("").to_lowercase() != "https" {
            return Err(eyre!("The Adobe endpoint must use HTTPS"));
        }
        if let ProxyMode::Cache | ProxyMode::Store | ProxyMode::Forward = self.proxy.mode
        {
            if self.cache.db_path.is_empty() {
                return Err(eyre!("Database path can't be empty when cache is enabled"));
            }
        }
        if self.network.use_proxy {
            if self.network.proxy_host.is_empty() {
                return Err(eyre!("Proxy host can't be empty"));
            }
            if self.network.proxy_host.contains(':') {
                return Err(eyre!(
                    "Proxy host must not contain a port (use the 'proxy_port' config option)"
                ));
            }
            if self.network.proxy_port.is_empty() {
                return Err(eyre!("Proxy port can't be empty"));
            }
            if self.network.use_basic_auth && self.network.proxy_username.is_empty() {
                return Err(eyre!(
                    "Proxy username can't be empty if proxy authentication is on"
                ));
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
            Err(eyre!(
                "proxy mode '{}' must be a prefix of cache, store, forward or passthrough",
                s
            ))
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
            Err(eyre!(
                "Log level '{}' must be a prefix of off, error, warn, info, debug, or trace",
                s
            ))
        }
    }
}
