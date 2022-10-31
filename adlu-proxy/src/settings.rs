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
use std::ffi::OsStr;
use std::fmt::{Debug, Formatter};
use std::fs::File;
use std::io::prelude::*;
use std::sync::Arc;

use config::{Config, Environment, File as ConfigFile, FileFormat};
use dialoguer::{Confirm, Input, Password, Select};
use eyre::{eyre, Report, Result, WrapErr};
use serde::{Deserialize, Serialize};

use crate::cli::{Command, ProxyArgs};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proxy {
    pub db_path: String,
    pub mode: ProxyMode,
    pub host: String,
    pub port: String,
    pub ssl_port: String,
    pub ssl: bool,
}

impl Default for Proxy {
    fn default() -> Self {
        Proxy {
            db_path: "proxy-cache.sqlite".to_string(),
            mode: Default::default(),
            host: "0.0.0.0".to_string(),
            port: "8080".to_string(),
            ssl_port: "8443".to_string(),
            ssl: false,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Ssl {
    pub use_pfx: bool,
    pub pfx_path: String,
    pub cert_path: String,
    pub key_path: String,
    pub password: String,
}

impl Default for Ssl {
    fn default() -> Self {
        Ssl {
            use_pfx: true,
            pfx_path: "proxy-certkey".to_string(),
            cert_path: "proxy-cert".to_string(),
            key_path: "proxy-key".to_string(),
            password: "".to_string(),
        }
    }
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
#[serde(rename_all = "lowercase")]
pub enum LogRotationType {
    None = 0,
    Daily = 1,
    Sized = 2,
}

impl std::fmt::Display for LogRotationType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LogRotationType::None => write!(f, "None"),
            LogRotationType::Daily => write!(f, "Daily"),
            LogRotationType::Sized => write!(f, "By Size"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Logging {
    pub level: LogLevel,
    pub destination: LogDestination,
    pub file_path: String,
    pub rotate_type: LogRotationType,
    pub rotate_size_kb: u64,
    pub rotate_count: u32,
}

impl Default for Logging {
    fn default() -> Self {
        Logging {
            level: LogLevel::Info,
            destination: LogDestination::File,
            file_path: "proxy-log.log".to_string(),
            rotate_type: LogRotationType::None,
            rotate_size_kb: 100,
            rotate_count: 10,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Upstream {
    pub use_proxy: bool,
    pub proxy_protocol: String,
    pub proxy_host: String,
    pub proxy_port: String,
    pub use_basic_auth: bool,
    pub proxy_username: String,
    pub proxy_password: String,
}

impl Default for Upstream {
    fn default() -> Self {
        Upstream {
            use_proxy: false,
            proxy_protocol: "http".to_string(),
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: "8888".to_string(),
            use_basic_auth: false,
            proxy_username: "".to_string(),
            proxy_password: "".to_string(),
        }
    }
}

impl Debug for Upstream {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Network")
            .field("use_proxy", &self.use_proxy)
            .field("proxy_protocol", &self.proxy_protocol)
            .field("proxy_host", &self.proxy_host)
            .field("proxy_port", &self.proxy_port)
            .field("use_proxy", &self.use_proxy)
            .field("proxy_username", &self.proxy_username)
            .field("proxy_password", &"[OBSCURED]")
            .finish()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Frl {
    pub remote_host: String,
}

impl Default for Frl {
    fn default() -> Self {
        Frl { remote_host: "https://lcs-cops.adobe.io".to_string() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Log {
    pub remote_host: String,
}

impl Default for Log {
    fn default() -> Self {
        Log { remote_host: "https://lcs-ulecs.adobe.io".to_string() }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SettingsVal {
    pub version: Option<u32>,
    pub proxy: Proxy,
    pub ssl: Ssl,
    pub frl: Frl,
    pub log: Log,
    pub upstream: Upstream,
    pub logging: Logging,
}

pub type Settings = Arc<SettingsVal>;

/// Obtain default settings
pub fn default_config() -> Settings {
    Settings::new(SettingsVal::default_config())
}

/// Load settings from the configuration file
pub fn load_config_file(args: &ProxyArgs) -> Result<Settings> {
    Ok(Settings::new(SettingsVal::load_config(args)?))
}

/// Update (or create) a configuration file after interviewing user
/// No logging on this path, because it might interfere with the interview
pub fn update_config_file(settings: Option<&Settings>, args: &ProxyArgs) -> Result<()> {
    // get the configuration
    let mut conf: SettingsVal = match settings {
        Some(settings) => settings.as_ref().clone(),
        None => SettingsVal::default_config(),
    };
    // maybe interview the user for updates
    let repair_only = matches!(args.cmd, Command::Configure { repair: true });
    if settings.is_none() || !repair_only {
        conf.update_config().wrap_err("Configuration interview failed")?;
    }
    // save the configuration
    let path = args.config_file.as_str();
    let toml = toml::to_string(&conf)
        .wrap_err(format!("Cannot serialize configuration: {:?}", &conf))?;
    let mut file =
        File::create(path).wrap_err(format!("Cannot create config file: {}", path))?;
    file.write_all(toml.as_bytes())
        .wrap_err(format!("Cannot write config file: {}", path))?;
    eprintln!("Wrote config file '{}'", path);
    Ok(())
}

impl SettingsVal {
    /// Create a new default config
    pub fn default_config() -> Self {
        Default::default()
    }

    /// Load an existing config file, returning its contained config
    pub fn load_config(args: &ProxyArgs) -> Result<Self> {
        let default_str = toml::to_string(&Self::default_config()).unwrap();
        let builder = Config::builder()
            .add_source(ConfigFile::from_str(&default_str, FileFormat::Toml))
            .add_source(ConfigFile::new(&args.config_file, FileFormat::Toml))
            .add_source(Environment::with_prefix("adlu_proxy"));
        let mut settings: Self = builder.build()?.try_deserialize()?;
        // Now repair the older config if needed
        settings.repair_config(args)?;
        // Now process the args as overrides: global first, then command-specific
        match args.debug {
            1 => settings.logging.level = LogLevel::Debug,
            n if n >= 2 => settings.logging.level = LogLevel::Trace,
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
            Command::Serve { mode, ssl } => {
                if let Some(mode) = mode {
                    settings.proxy.mode = mode.as_str().try_into()?;
                }
                if let Some(ssl) = ssl {
                    settings.proxy.ssl = *ssl;
                }
            }
            Command::Clear { .. }
            | Command::Import { .. }
            | Command::Export { .. }
            | Command::Report { .. }
            | Command::Forward => {
                // log to file, because these commands are interactive
                if !matches!(settings.logging.level, LogLevel::Off) {
                    settings.logging.destination = LogDestination::File
                };
            }
            Command::Configure { .. } => {
                // allow repair, because we're configuring
                // don't touch the settings, so they can be configured
            }
        }
        Ok(settings)
    }

    fn version_none_to_one(&mut self) {
        // in version None, the only kind of log rotation was by size
        self.version = Some(1);
        if self.logging.rotate_count > 0 && self.logging.rotate_size_kb > 0 {
            self.logging.rotate_type = LogRotationType::Sized
        }
    }

    fn repair_config(&mut self, args: &ProxyArgs) -> Result<()> {
        let can_repair = matches!(args.cmd, Command::Configure { .. });
        let mut did_repair = false;
        if self.version.is_none() {
            did_repair = true;
            self.version_none_to_one();
        }
        if did_repair && !can_repair {
            Err(eyre!("Please reconfigure"))
        } else {
            Ok(())
        }
    }

    /// Update configuration settings by interviewing user
    /// No logging on this path, because it might interfere with the interview
    pub fn update_config(&mut self) -> Result<()> {
        self.update_proxy_config()?;
        self.update_ssl_config()?;
        self.update_frl_config()?;
        self.update_log_config()?;
        self.update_upstream_config()?;
        self.update_logging_config()?;
        Ok(())
    }

    fn update_proxy_config(&mut self) -> Result<()> {
        eprintln!("The proxy uses a database to keep track of requests and responses.");
        eprintln!(
            "This database will be created if it doesn't exist when the proxy starts."
        );
        self.proxy.db_path = Input::new()
            .allow_empty(false)
            .with_prompt("Name of (or path to) your database file")
            .with_initial_text(&self.proxy.db_path)
            .interact_text()?;
        eprintln!("The proxy has three modes: transparent, connected, and isolated.");
        eprintln!("Read the user guide to understand which is right for each situation.");
        let choices = vec!["transparent", "connected", "isolated"];
        let default = self.proxy.mode.clone() as usize;
        let choice = Select::new()
            .items(&choices)
            .default(default)
            .with_prompt("Proxy Mode")
            .interact()?;
        self.proxy.mode = choices[choice].try_into().unwrap();
        eprintln!("You must specify a numeric IPv4 address for the proxy to listen on.");
        eprintln!("Use 0.0.0.0 to have the proxy listen on all available addresses.");
        let choice: String = Input::new()
            .with_prompt("Numeric IPv4 address")
            .with_initial_text(&self.proxy.host)
            .validate_with(host_validator)
            .interact_text()?;
        self.proxy.host = choice;
        let choice: String = Input::new()
            .with_prompt("Host port for http (non-ssl) mode")
            .with_initial_text(&self.proxy.port)
            .validate_with(port_validator)
            .interact_text()?;
        self.proxy.port = choice;
        Ok(())
    }

    fn update_ssl_config(&mut self) -> Result<()> {
        self.proxy.ssl = Confirm::new()
            .default(self.proxy.ssl)
            .show_default(true)
            .wait_for_newline(false)
            .with_prompt("Use SSL?")
            .interact()?;
        if !self.proxy.ssl {
            return Ok(());
        }
        self.proxy.ssl_port = Input::new()
            .with_prompt("Host port for https mode")
            .with_initial_text(&self.proxy.ssl_port)
            .validate_with(port_validator)
            .interact_text()?;
        eprintln!("The proxy requires a certificate and matching key.");
        eprintln!("You can either use separate certificate and key files, or");
        eprintln!("you can use a combined PKCS12 (aka PFX) file that has both.");
        eprintln!("The user guide has information or preparing these files.");
        let choices = [
            "Use a single PKCS12/PFX file (in DER format)",
            "Use separate cert and key files (in PEM format)",
        ];
        let choice = Select::new()
            .items(&choices)
            .default(if self.ssl.use_pfx { 0 } else { 1 })
            .with_prompt("How will you supply your certificate and key")
            .interact()?;
        if choice == 0 {
            self.ssl.use_pfx = true;
            self.ssl.pfx_path =
                get_existing_file_path("PKCS12", &self.ssl.cert_path, "pfx")?;
        } else {
            self.ssl.use_pfx = false;
            self.ssl.cert_path =
                get_existing_file_path("certificate", &self.ssl.cert_path, "cert")?;
            self.ssl.key_path = get_existing_file_path("key", &self.ssl.key_path, "key")?;
        }
        eprintln!("Files containing keys are usually encrypted with a password.");
        eprintln!("Your proxy requires that password in order to function properly.");
        eprintln!("You can keep your password either in your config file or");
        eprintln!("in an environment variable named ADLU_PROXY_SSL.PASSWORD");
        let prompt = if self.ssl.password.is_empty() {
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
            self.ssl.password = choice;
        }
        Ok(())
    }

    fn update_frl_config(&mut self) -> Result<()> {
        eprintln!("Your proxy server must contact one of two Adobe licensing servers.");
        eprintln!("Use the variable IP server unless your firewall doesn't permit it.");
        let choices = vec![
            "Variable IP server (lcs-cops.adobe.io)",
            "Fixed IP server (lcs-cops-proxy.adobe.com)",
        ];
        let default = if self.frl.remote_host == "https://lcs-cops-proxy.adobe.com" {
            1
        } else {
            0
        };
        let choice = Select::new()
            .items(&choices)
            .default(default)
            .with_prompt("Adobe licensing server")
            .interact()?;
        self.frl.remote_host = if choice == 0usize {
            String::from("https://lcs-cops.adobe.io")
        } else {
            String::from("https://lcs-cops-proxy.adobe.com")
        };
        Ok(())
    }

    fn update_log_config(&mut self) -> Result<()> {
        self.log.remote_host = String::from("https://lcs-ulecs.adobe.io");
        Ok(())
    }

    fn update_upstream_config(&mut self) -> Result<()> {
        // update network settings
        let prompt = "Does your network require this proxy to use an upstream proxy?";
        let choice = Confirm::new()
            .default(self.upstream.use_proxy)
            .wait_for_newline(false)
            .with_prompt(prompt)
            .interact()?;
        self.upstream.use_proxy = choice;
        if choice {
            let choice: String = Input::new()
                .with_prompt("Proxy host")
                .with_initial_text(&self.upstream.proxy_host)
                .interact_text()?;
            self.upstream.proxy_host = choice;
            let choice: String = Input::new()
                .with_prompt("Proxy port")
                .with_initial_text(&self.upstream.proxy_port)
                .interact_text()?;
            self.upstream.proxy_port = choice;
            let prompt = "Does your upstream proxy require (basic) authentication?";
            let choice = Confirm::new()
                .default(self.upstream.use_basic_auth)
                .wait_for_newline(false)
                .with_prompt(prompt)
                .interact()?;
            self.upstream.use_basic_auth = choice;
            if choice {
                let choice: String = Input::new()
                    .with_prompt("Proxy username")
                    .with_initial_text(&self.upstream.proxy_username)
                    .interact_text()?;
                self.upstream.proxy_username = choice;
                let choice: String = Input::new()
                    .with_prompt("Proxy password")
                    .with_initial_text(&self.upstream.proxy_password)
                    .interact_text()?;
                self.upstream.proxy_password = choice;
            }
        }
        Ok(())
    }

    fn update_logging_config(&mut self) -> Result<()> {
        // update log settings
        let prompt = if let LogLevel::Off = self.logging.level {
            // defensively set log destination to console when logging is off
            // to avoid problems with manually configured log files.
            self.logging.destination = LogDestination::Console;
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
            let log_level_info = match self.logging.level {
                LogLevel::Off | LogLevel::Info => {
                    self.logging.level = LogLevel::Info;
                    true
                }
                _ => false,
            };
            let do_configure = !log_level_info || {
                eprintln!("The proxy will log errors, warnings and summary information.");
                Confirm::new()
                    .default(false)
                    .wait_for_newline(false)
                    .with_prompt("Do you want to adjust the level of logged information?")
                    .interact()?
            };
            if do_configure {
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
            if matches!(self.logging.level, LogLevel::Off) {
                // if there is no logging, use the console, so we don't create an empty log file
                self.logging.destination = LogDestination::Console;
            } else {
                eprintln!("The proxy can log to the console (standard output) or to a file on disk.");
                let choices = vec!["console", "disk file"];
                let choice = Select::new()
                    .items(&choices)
                    .default(1)
                    .with_prompt("Log destination")
                    .interact()?;
                self.logging.destination = if choice == 0 {
                    LogDestination::Console
                } else {
                    LogDestination::File
                };
                if choice == 1 {
                    let choice: String = Input::new()
                        .allow_empty(false)
                        .with_prompt("Name of (or path to) your log file")
                        .with_initial_text(&self.logging.file_path)
                        .interact_text()?;
                    self.logging.file_path = choice;
                }
            }
            // ask about log rotation
            if matches!(self.logging.destination, LogDestination::File) {
                let prompt = if let LogRotationType::None = self.logging.rotate_type {
                    eprintln!("The proxy is not doing log rotation.");
                    "Do you want to enable log rotation?"
                } else {
                    if let LogRotationType::Sized = self.logging.rotate_type {
                        eprintln!(
                            "The proxy is rotating logs when they reach {}KB.",
                            self.logging.rotate_size_kb
                        );
                    } else {
                        eprintln!("The proxy is rotating logs every day");
                    }
                    "Do you want to change your log rotation configuration?"
                };
                let choice = Confirm::new()
                    .default(false)
                    .wait_for_newline(false)
                    .with_prompt(prompt)
                    .interact()?;
                if choice {
                    let choices = [
                        LogRotationType::None.to_string(),
                        LogRotationType::Daily.to_string(),
                        LogRotationType::Sized.to_string(),
                    ];
                    let default = self.logging.rotate_type.clone() as usize;
                    let choice = Select::new()
                        .items(&choices)
                        .default(default)
                        .with_prompt("Rotation type")
                        .interact()?;
                    if choice == 0 {
                        self.logging.rotate_type = LogRotationType::None;
                    } else {
                        self.logging.rotate_count = Input::new()
                            .default(self.logging.rotate_count)
                            .validate_with(|cnt: &u32| {
                                if *cnt > 0 && *cnt < 100 {
                                    Ok(())
                                } else {
                                    Err(eyre!("Value must be between 0 and 100"))
                                }
                            })
                            .with_prompt("Keep this many log files (1-99)")
                            .interact()?;
                        if choice == 1 {
                            self.logging.rotate_type = LogRotationType::Daily;
                        } else {
                            self.logging.rotate_type = LogRotationType::Sized;
                            self.logging.rotate_size_kb = Input::new()
                                .default(self.logging.rotate_size_kb)
                                .validate_with(|cnt: &u64| {
                                    if *cnt > 0 {
                                        Ok(())
                                    } else {
                                        Err(eyre!("Value must be greater than 0"))
                                    }
                                })
                                .with_prompt("Max log file size in KB")
                                .interact()?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[allow(clippy::ptr_arg)]
fn host_validator(s: &String) -> Result<()> {
    let s = s.as_str();
    if s.contains(':') {
        return Err(eyre!("Do not specify a port, just an IPv4 address"));
    }
    let s = format!("{}:0", s);
    let val = s.parse::<std::net::SocketAddr>();
    match val {
        Ok(_) => Ok(()),
        Err(_) => Err(eyre!("Specify a valid IPv4 numeric address (e.g. 127.0.0.1)")),
    }
}

#[allow(clippy::ptr_arg)]
fn port_validator(s: &String) -> Result<()> {
    match s.parse::<u32>() {
        Ok(p) if p > 0 && p < 65_536 => Ok(()),
        Ok(_) => Err(eyre!("Port must be between 0 and 65536")),
        Err(_) => Err(eyre!("Port must be a number")),
    }
}

fn get_existing_file_path(
    prompt: &str,
    initial: &str,
    extension: &str,
) -> Result<String> {
    let ext = std::path::Path::new(initial).extension().and_then(OsStr::to_str);
    let prompt = format!("Name of (or path to) your {} file", prompt);
    let mut choice = match ext {
        None => format!("{}.{}", initial, extension),
        Some(_) => initial.to_string(),
    };
    loop {
        choice = Input::new()
            .with_prompt(&prompt)
            .with_initial_text(choice)
            .interact_text()?;
        if std::fs::metadata(&choice).is_ok() {
            break;
        } else {
            eprintln!("There is no file at that path, try again.");
        }
    }
    Ok(choice)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    Transparent,
    Connected,
    Isolated,
}

impl Default for ProxyMode {
    fn default() -> Self {
        ProxyMode::Connected
    }
}

impl TryFrom<&str> for ProxyMode {
    type Error = Report;

    fn try_from(s: &str) -> Result<Self> {
        let sl = s.to_ascii_lowercase();
        if "transparent".starts_with(&sl) {
            Ok(ProxyMode::Transparent)
        } else if "connected".starts_with(&sl) {
            Ok(ProxyMode::Connected)
        } else if "isolated".starts_with(&sl) {
            Ok(ProxyMode::Isolated)
        } else {
            Err(eyre!(
                "FRL mode '{}' must be a prefix of transparent, connected, or isolated",
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
