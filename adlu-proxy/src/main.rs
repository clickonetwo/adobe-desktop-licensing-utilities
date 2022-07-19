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
use clap::Parser;
use eyre::{Result, WrapErr};
use log::debug;

use cache::Cache;
use cli::{Command, FrlProxy};

use crate::settings::ProxyMode;

mod api;
mod cache;
mod cli;
mod handlers;
mod logging;
mod proxy;
mod settings;

#[tokio::main]
async fn main() -> Result<()> {
    openssl_probe::init_ssl_cert_env_vars();
    let args: FrlProxy = FrlProxy::parse();

    // if we have a valid config, proceed, else update the config
    if let Ok(settings) = settings::load_config_file(&args) {
        debug!("Loaded config: {:?}", &settings);
        match args.cmd {
            Command::Start { .. } => {
                logging::init(&settings)?;
                if let ProxyMode::Forward = settings.proxy.mode {
                    let cache = Cache::from(&settings, false).await?;
                    proxy::forward_stored_requests(settings.clone(), cache.clone()).await;
                    cache.close().await;
                } else {
                    let cache = Cache::from(&settings, true).await?;
                    if settings.proxy.ssl {
                        proxy::serve_incoming_https_requests(
                            settings.clone(),
                            cache.clone(),
                        )
                        .await;
                    } else {
                        proxy::serve_incoming_http_requests(
                            settings.clone(),
                            cache.clone(),
                        )
                        .await;
                    }
                    cache.close().await;
                }
            }
            Command::Clear { yes } => {
                logging::init(&settings)?;
                let cache = Cache::from(&settings, true).await?;
                cache.clear(yes).await.wrap_err("Failed to clear cache")?;
                cache.close().await;
            }
            Command::Import { import_path } => {
                logging::init(&settings)?;
                let cache = Cache::from(&settings, true).await?;
                cache
                    .import(&import_path)
                    .await
                    .wrap_err(format!("Failed to import from {}", &import_path))?;
                cache.close().await;
            }
            Command::Export { export_path } => {
                logging::init(&settings)?;
                let cache = Cache::from(&settings, false).await?;
                cache
                    .export(&export_path)
                    .await
                    .wrap_err(format!("Failed to export to {}", &export_path))?;
                cache.close().await;
            }
            Command::Configure => {
                // no logging on this path, because it might interfere with the interview
                settings::update_config_file(Some(&settings), &args.config_file)?;
            }
        }
    } else {
        eprintln!("Couldn't read the configuration file, creating a new one...");
        settings::update_config_file(None, &args.config_file)
            .wrap_err("Failed to update configuration file")?;
    }
    Ok(())
}
