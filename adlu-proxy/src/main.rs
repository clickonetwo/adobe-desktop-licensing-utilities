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

use adlu_proxy::cli::{Command, ProxyArgs};
use adlu_proxy::{cache, logging, proxy, settings};

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("Proxy failure: {}", err);
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let args: ProxyArgs = ProxyArgs::parse();
    // if we have a valid config, proceed, else update the config
    if let Ok(settings) = settings::load_config_file(&args) {
        logging::init(&settings)?;
        debug!("Loaded config: {:?}", &settings);
        let cache = cache::connect(&settings).await?;
        let result = match args.cmd {
            Command::Configure => {
                settings::update_config_file(Some(&settings), &args.config_file)
            }
            Command::Serve { .. } => {
                if settings.proxy.ssl {
                    proxy::serve_incoming_https_requests(&settings, &cache).await
                } else {
                    proxy::serve_incoming_http_requests(&settings, &cache).await
                }
            }
            Command::Forward => proxy::forward_stored_requests(&settings, &cache).await,
            Command::Clear { yes } => {
                cache.clear(yes).await.wrap_err("Failed to clear cache")
            }
            Command::Import { from_path: import_path } => cache
                .import(&import_path)
                .await
                .wrap_err(format!("Failed to import from {}", &import_path)),
            Command::Export { to_path: export_path } => cache
                .export(&export_path)
                .await
                .wrap_err(format!("Failed to export to {}", &export_path)),
            Command::Report { to_path: report_path } => cache
                .report(&report_path)
                .await
                .wrap_err(format!("Failed to report to {}", &report_path)),
        };
        cache.close().await;
        result?;
    } else {
        eprintln!("Couldn't read the configuration file, creating a new one...");
        settings::update_config_file(None, &args.config_file)
            .wrap_err("Failed to update configuration file")?;
    }
    Ok(())
}
