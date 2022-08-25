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
use eyre::{Result, WrapErr};
use log::debug;

use cli::{Command, ProxyArgs};
use settings::Settings;

pub mod cache;
pub mod cli;
pub mod logging;
pub mod proxy;
pub mod settings;
#[cfg(test)]
pub mod testing;

pub async fn run(
    settings: Settings,
    args: ProxyArgs,
    stop_signal: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<()> {
    logging::init(&settings)?;
    debug!("Loaded config: {:?}", &settings);
    let cache = cache::connect(&settings).await?;
    let result = match args.cmd {
        Command::Configure => {
            settings::update_config_file(Some(&settings), &args.config_file)
        }
        Command::Serve { .. } => {
            if settings.proxy.ssl {
                proxy::serve_incoming_https_requests(&settings, &cache, stop_signal).await
            } else {
                proxy::serve_incoming_http_requests(&settings, &cache, stop_signal).await
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
    result
}

#[cfg(test)]
mod tests {
    use super::proxy;
    use super::testing::*;

    #[tokio::test]
    async fn test_mock_requests() {
        let conf = test_config("proxy-mock-requests").await;
        tokio::join!(
            test_activation_request_success(conf.clone()),
            test_deactivation_request_success(conf.clone()),
            test_log_upload_request_success(conf.clone()),
        );
    }

    async fn test_activation_request_success(conf: proxy::Config) {
        let filter = proxy::activate_route(conf);
        let mut builder = warp::test::request();
        builder = mock_activation_request(&MockOutcome::Success, builder);
        let response = builder.reply(&filter).await;
        assert_eq!(response.status().as_u16(), 200);
    }

    async fn test_deactivation_request_success(conf: proxy::Config) {
        let filter = proxy::deactivate_route(conf);
        let mut builder = warp::test::request();
        builder = mock_deactivation_request(&MockOutcome::Success, builder);
        let response = builder.reply(&filter).await;
        assert_eq!(response.status().as_u16(), 200);
    }

    async fn test_log_upload_request_success(conf: proxy::Config) {
        let filter = proxy::upload_route(conf);
        let mut builder = warp::test::request();
        builder = mock_log_upload_request(&MockOutcome::Success, builder);
        let response = builder.reply(&filter).await;
        assert_eq!(response.status().as_u16(), 200);
    }
}
