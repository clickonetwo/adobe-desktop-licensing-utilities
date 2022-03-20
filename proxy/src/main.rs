/*
Copyright 2020 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
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
