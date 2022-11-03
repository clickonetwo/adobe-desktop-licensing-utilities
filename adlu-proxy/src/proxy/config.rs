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
use eyre::{eyre, Context, Result};

use adlu_base::{load_pem_files, load_pfx_file, CertificateData};

use crate::cache::Cache;
use crate::settings::Settings;

#[derive(Debug, Clone)]
pub struct Config {
    pub settings: Settings,
    pub cache: Cache,
    pub client: reqwest::Client,
    pub frl_server: String,
    pub log_server: String,
}

impl Config {
    pub fn new(settings: Settings, cache: Cache) -> Result<Self> {
        let mut builder = reqwest::Client::builder();
        builder = builder.timeout(std::time::Duration::new(59, 0));
        if settings.upstream.use_proxy {
            let proxy_host = format!(
                "{}://{}:{}",
                settings.upstream.proxy_protocol,
                settings.upstream.proxy_host,
                settings.upstream.proxy_port
            );
            let mut proxy = reqwest::Proxy::https(&proxy_host)
                .wrap_err("Invalid proxy configuration")?;
            if settings.upstream.use_basic_auth {
                proxy = proxy.basic_auth(
                    &settings.upstream.proxy_username,
                    &settings.upstream.proxy_password,
                );
            }
            builder = builder.proxy(proxy)
        }
        let client = builder.build().wrap_err("Can't create proxy client")?;
        let frl_server: http::Uri =
            settings.frl.remote_host.parse().wrap_err("Invalid FRL endpoint")?;
        let log_server: http::Uri =
            settings.log.remote_host.parse().wrap_err("Invalid log endpoint")?;
        Ok(Config {
            settings,
            cache,
            client,
            frl_server: frl_server.to_string(),
            log_server: log_server.to_string(),
        })
    }

    #[cfg(test)]
    pub fn clone_with_mode(&self, mode: &crate::settings::ProxyMode) -> Self {
        let mut new_settings = self.settings.as_ref().clone();
        new_settings.proxy.mode = mode.clone();
        let mut new_config = self.clone();
        new_config.settings = Settings::new(new_settings);
        new_config
    }

    pub fn bind_addr(&self) -> Result<std::net::SocketAddr> {
        let proxy_addr = if self.settings.proxy.ssl {
            format!("{}:{}", self.settings.proxy.host, self.settings.proxy.ssl_port)
        } else {
            format!("{}:{}", self.settings.proxy.host, self.settings.proxy.port)
        };
        proxy_addr.parse().wrap_err("Invalid proxy host/port configuration")
    }

    pub fn cert_data(&self) -> Result<CertificateData> {
        if self.settings.proxy.ssl {
            load_cert_data(&self.settings).wrap_err("SSL configuration failure")
        } else {
            Err(eyre!("SSL is not enabled"))
        }
    }
}

fn load_cert_data(settings: &Settings) -> Result<CertificateData> {
    if settings.ssl.use_pfx {
        load_pfx_file(&settings.ssl.cert_path, &settings.ssl.password)
            .wrap_err("Failed to load PKCS12 data:")
    } else {
        let key_pass = match settings.ssl.password.as_str() {
            "" => None,
            p => Some(p),
        };
        load_pem_files(&settings.ssl.key_path, &settings.ssl.cert_path, key_pass)
            .wrap_err("Failed to load certificate and key files")
    }
}
