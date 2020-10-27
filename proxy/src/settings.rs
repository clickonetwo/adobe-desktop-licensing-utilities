/*
 * MIT License
 *
 * Copyright (c) 2020 Adobe, Inc.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */
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
pub struct Cache {
    pub enabled: Option<bool>,
    pub cache_file_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Settings {
    pub proxy: Proxy,
    pub logging: Logging,
    pub cache: Cache,
}

impl Settings {
    pub fn from_start(config_file: Option<String>, host: Option<String>, remote_host: Option<String>,
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

    pub fn from_cache_control(config_file: Option<String>) -> Result<Self, ConfigError> {
        let mut s = Config::new();
        s.merge(ConfigFile::from_str(include_str!("res/defaults.toml"), FileFormat::Toml))?;
        if let Some(filename) = config_file {
            s.merge(ConfigFile::with_name(filename.as_str()))?;
        }
        s.try_into()
    }

    pub fn validate(self: &Self) -> Result<()> {
        if let Some(true) = self.proxy.ssl {
            if self.proxy.ssl_cert.is_none() || self.proxy.ssl_key.is_none() {
                return Err(eyre!("ssl_cert and ssl_key must be specified if SSL is enabled"));
            } else if self.proxy.ssl_cert.as_ref().unwrap().is_empty() {
                return Err(eyre!("ssl_cert pathname cannot be an empty string"));
            } else if self.proxy.ssl_key.as_ref().unwrap().is_empty() {
                return Err(eyre!("ssl_key pathname cannot be an empty string"));
            }
        }
        if let Some(true) = self.cache.enabled {
            if self.cache.cache_file_path.is_none() {
                return Err(eyre!("cache_file_path must be specified if the cache is enabled"));
            } else if self.cache.cache_file_path.as_ref().unwrap().is_empty() {
                return Err(eyre!("The cache_file_path cannot be an empty string"))
            }
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
