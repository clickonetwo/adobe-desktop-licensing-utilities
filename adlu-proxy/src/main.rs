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

mod cache;
mod cli;
mod cops;
mod logging;
mod proxy;
mod settings;

use crate::cli::Command;
use crate::settings::{LogDestination, ProxyMode};
use cache::Cache;
use cli::FrlProxy;
use eyre::{Result, WrapErr};
use log::debug;
use proxy::{plain, secure};
use settings::Settings;
use std::convert::TryInto;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    openssl_probe::init_ssl_cert_env_vars();
    let args: FrlProxy = FrlProxy::parse();

    // make sure we have a config file.  if not, make one
    if let Some(mut conf) = Settings::load_config(&args)? {
        match args.cmd {
            cli::Command::Start { mode, ssl } => {
                if let Some(mode) = mode {
                    conf.proxy.mode = mode.as_str().try_into()?;
                };
                if let Some(ssl) = ssl {
                    conf.proxy.ssl = ssl;
                }
                conf.validate()?;
                logging::init(&conf)?;
                debug!("conf: {:?}", conf);
                if let ProxyMode::Forward = conf.proxy.mode {
                    let cache = Cache::from(&conf, false).await?;
                    proxy::forward_stored_requests(&conf, Arc::clone(&cache)).await;
                    cache.close().await;
                } else {
                    let cache = Cache::from(&conf, true).await?;
                    if conf.proxy.ssl {
                        secure::run_server(&conf, Arc::clone(&cache)).await?;
                    } else {
                        plain::run_server(&conf, Arc::clone(&cache)).await?;
                    }
                    cache.close().await;
                }
            }
            cli::Command::Configure => {
                conf.validate()?;
                // do not log configuration changes, because
                // logging might interfere with the interactions
                // and there really isn't anything to log.
                conf.update_config_file(&args.config_file)?;
            }
            Command::Clear { yes } => {
                conf.proxy.mode = ProxyMode::Cache;
                // log to file, because this command is interactive
                conf.logging.destination = LogDestination::File;
                conf.validate()?;
                logging::init(&conf)?;
                let cache = Cache::from(&conf, true).await?;
                cache.clear(yes).await.wrap_err("Failed to clear cache")?;
            }
            Command::Import { import_path } => {
                conf.proxy.mode = ProxyMode::Cache;
                // log to file, because this command is interactive
                conf.logging.destination = LogDestination::File;
                conf.validate()?;
                logging::init(&conf)?;
                let cache = Cache::from(&conf, true).await?;
                cache
                    .import(&import_path)
                    .await
                    .wrap_err(format!("Failed to import from {}", &import_path))?;
            }
            Command::Export { export_path } => {
                conf.proxy.mode = ProxyMode::Cache;
                // log to file, because this command is interactive
                conf.logging.destination = LogDestination::File;
                conf.validate()?;
                logging::init(&conf)?;
                let cache = Cache::from(&conf, false).await?;
                cache
                    .export(&export_path)
                    .await
                    .wrap_err(format!("Failed to export to {}", &export_path))?;
            }
        }
    } else {
        let mut conf = Settings::load_config(&args)?.unwrap();
        conf.update_config_file(&args.config_file)
            .wrap_err("Failed to update configuration file")?;
    }
    Ok(())
}
