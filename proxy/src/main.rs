/*
Copyright 2020 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
use structopt::StructOpt;

mod cache;
mod cli;
mod cops;
mod logging;
mod proxy;
mod settings;

use cache::Cache;
use cli::Opt;
use proxy::{plain, secure};
use settings::Settings;

use log::debug;
use crate::settings::ProxyMode;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    openssl_probe::init_ssl_cert_env_vars();
    let opt = Opt::from_args();

    match opt {
        cli::Opt::Start {
            config_file,
            mode,
            host,
            remote_host,
            ssl,
            ssl_cert,
            ssl_key,
        } => {
            let conf = Settings::from_start(
                config_file,
                mode,
                host,
                remote_host,
                ssl,
                ssl_cert,
                ssl_key,
            )?;
            conf.validate()?;
            logging::init(&conf)?;
            let cache = Cache::new_from(&conf).await?;
            debug!("conf: {:?}", conf);
            if let ProxyMode::Forward = conf.proxy.mode {
                proxy::forward_stored_requests(&conf, cache).await;
            } else if let Some(true) = conf.proxy.ssl {
                secure::run_server(&conf, cache).await?;
            } else {
                plain::run_server(&conf, cache).await?;
            }
        }
        cli::Opt::InitConfig { out_file } => {
            settings::config_template(out_file)?;
            std::process::exit(0);
        }
        cli::Opt::CacheControl {
            config_file,
            clear,
            export_file,
            import_file,
        } => {
            let conf = Settings::from_cache_control(config_file)?;
            conf.validate()?;
            let cache = Cache::new_from(&conf).await?;
            Cache::control(&cache, clear, export_file, import_file).await?;
            debug!("conf: {:?}", conf);
        }
    }
    Ok(())
}
