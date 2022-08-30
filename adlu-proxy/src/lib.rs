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
    logging::init(&settings.logging)?;
    debug!("Loaded config: {:?}", &settings);
    let cache = cache::connect(&settings.proxy.db_path).await?;
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
    use super::settings::ProxyMode;
    use super::testing::*;

    async fn send_activation(
        conf: &proxy::Config,
        outcome: &MockOutcome,
        device_id: &str,
    ) -> u16 {
        let filter = proxy::activate_route(conf.clone());
        let mut builder = warp::test::request();
        builder = mock_activation_request(outcome, device_id, builder);
        let response = builder.reply(&filter).await;
        response.status().as_u16()
    }

    async fn send_deactivation(
        conf: &proxy::Config,
        outcome: &MockOutcome,
        device_id: &str,
    ) -> u16 {
        let filter = proxy::deactivate_route(conf.clone());
        let mut builder = warp::test::request();
        builder = mock_deactivation_request(outcome, device_id, builder);
        let response = builder.reply(&filter).await;
        response.status().as_u16()
    }

    async fn send_log_upload(
        conf: &proxy::Config,
        outcome: &MockOutcome,
        session_id: &str,
    ) -> u16 {
        let filter = proxy::upload_route(conf.clone());
        let mut builder = warp::test::request();
        builder = mock_log_upload_request(outcome, session_id, builder);
        let response = builder.reply(&filter).await;
        response.status().as_u16()
    }

    #[tokio::test]
    async fn test_activation_request() {
        let conf = get_test_config(&ProxyMode::Connected).await;
        let result = send_activation(&conf, &MockOutcome::Success, "ar1").await;
        assert_eq!(result, 200);
        release_test_config(conf).await;
    }

    #[tokio::test]
    async fn test_activation_cache() {
        let conf = get_test_config(&ProxyMode::Isolated).await;
        let result = send_activation(&conf, &MockOutcome::Isolated, "ac1").await;
        assert_eq!(result, 502);
        let conf = conf.clone_with_mode(&ProxyMode::Connected);
        let result = send_activation(&conf, &MockOutcome::Success, "ac1").await;
        assert_eq!(result, 200);
        let conf = conf.clone_with_mode(&ProxyMode::Isolated);
        let result = send_activation(&conf, &MockOutcome::Isolated, "ac1").await;
        assert_eq!(result, 200);
        release_test_config(conf).await;
    }

    #[tokio::test]
    async fn test_deactivation_request() {
        let conf = get_test_config(&ProxyMode::Connected).await;
        let result = send_deactivation(&conf, &MockOutcome::Success, "dr1").await;
        assert_eq!(result, 200);
        release_test_config(conf).await;
    }

    #[tokio::test]
    async fn test_deactivation_cache() {
        let conf = get_test_config(&ProxyMode::Isolated).await;
        let result = send_deactivation(&conf, &MockOutcome::Isolated, "dc1").await;
        assert_eq!(result, 502);
        let conf = conf.clone_with_mode(&ProxyMode::Connected);
        let result = send_deactivation(&conf, &MockOutcome::Success, "dc1").await;
        assert_eq!(result, 200);
        let conf = conf.clone_with_mode(&ProxyMode::Isolated);
        let result = send_deactivation(&conf, &MockOutcome::Isolated, "dc1").await;
        assert_eq!(result, 200);
        release_test_config(conf).await;
    }

    #[tokio::test]
    async fn test_activation_deactivation_sequence() {
        let conf = get_test_config(&ProxyMode::Connected).await;
        let result = send_activation(&conf, &MockOutcome::Success, "ads1").await;
        assert_eq!(result, 200);
        let result = send_deactivation(&conf, &MockOutcome::Success, "ads1").await;
        assert_eq!(result, 200);
        let conf = conf.clone_with_mode(&ProxyMode::Isolated);
        let result = send_activation(&conf, &MockOutcome::Isolated, "ads1").await;
        assert_eq!(result, 502);
        release_test_config(conf).await;
    }

    #[tokio::test]
    async fn test_log_upload_request() {
        let conf = get_test_config(&ProxyMode::Connected).await;
        let result = send_log_upload(&conf, &MockOutcome::Success, "lr1").await;
        assert_eq!(result, 200);
        release_test_config(conf).await;
    }

    #[tokio::test]
    async fn test_log_upload_report() {
        let tempdir = get_test_directory();
        let conf = get_test_config(&ProxyMode::Connected).await;
        let result = send_log_upload(&conf, &MockOutcome::Success, "lrr1").await;
        assert_eq!(result, 200);
        let path = tempdir.join("test-report1.csv");
        conf.cache.report(path.to_str().unwrap()).await.expect("Report failed");
        let content = std::fs::read_to_string(&path).expect("Can't read report");
        assert!(content.contains("lrr1"));
    }
}
