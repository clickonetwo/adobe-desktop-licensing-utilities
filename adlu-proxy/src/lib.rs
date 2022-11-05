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
        Command::Configure { .. } => settings::update_config_file(Some(&settings), &args),
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
        Command::Import { data: source, from_path: import_path } => cache
            .import(&source, &import_path)
            .await
            .wrap_err(format!("Failed to import {} from {}", &source, &import_path)),
        Command::Export { data: source, to_path: export_path } => cache
            .export(&source, &export_path)
            .await
            .wrap_err(format!("Failed to export {} to {}", &source, &export_path)),
        Command::Report {
            data: source,
            empty,
            timezone,
            rfc3339,
            to_path: report_path,
        } => cache
            .report(&source, &report_path, empty, timezone, rfc3339)
            .await
            .wrap_err(format!("Failed to report {} to {}", &source, &report_path)),
    };
    cache.close().await;
    result
}

#[cfg(test)]
mod tests {
    use super::proxy;
    use super::settings::ProxyMode;
    use super::testing::*;
    use crate::cli::Datasource;

    async fn send_frl_activation(
        conf: &proxy::Config,
        outcome: &MockOutcome,
        device_id: &str,
    ) -> u16 {
        let filter = proxy::frl_activate_route(conf.clone());
        let mut builder = warp::test::request();
        builder = frl::mock_activation_request(outcome, device_id, builder);
        let response = builder.reply(&filter).await;
        response.status().as_u16()
    }

    async fn send_frl_deactivation(
        conf: &proxy::Config,
        outcome: &MockOutcome,
        device_id: &str,
    ) -> u16 {
        let filter = proxy::frl_deactivate_route(conf.clone());
        let mut builder = warp::test::request();
        builder = frl::mock_deactivation_request(outcome, device_id, builder);
        let response = builder.reply(&filter).await;
        response.status().as_u16()
    }

    async fn send_nul_activation(
        conf: &proxy::Config,
        outcome: &MockOutcome,
        device_id: &str,
    ) -> u16 {
        let filter = proxy::nul_license_route(conf.clone());
        let mut builder = warp::test::request();
        builder = named_user::mock_activation_request(outcome, device_id, builder);
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
        builder = log::mock_log_upload_request(outcome, session_id, builder);
        let response = builder.reply(&filter).await;
        response.status().as_u16()
    }

    #[tokio::test]
    async fn test_frl_activation_request() {
        let conf = get_test_config(&ProxyMode::Connected).await;
        let result = send_frl_activation(&conf, &MockOutcome::Success, "ar1").await;
        assert_eq!(result, 200);
        release_test_config(conf).await;
    }

    // we don't want to test round trips to Adobe servers as part of general library testing,
    // only explicitly while developing when we think we may have broken it.
    #[cfg(feature = "test_to_adobe")]
    #[tokio::test]
    async fn test_frl_activation_deactivation_to_adobe() {
        // this device_id is the sha256 of the NIST test string "abc"
        let device_id =
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let conf = get_test_config(&ProxyMode::Transparent).await;
        let result = send_frl_activation(&conf, &MockOutcome::FromAdobe, device_id).await;
        assert_eq!(result, 200);
        // give the server database time to replicate
        tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
        let result =
            send_frl_deactivation(&conf, &MockOutcome::FromAdobe, device_id).await;
        assert_eq!(result, 200);
        release_test_config(conf).await;
    }

    #[tokio::test]
    async fn test_frl_activation_cache() {
        let conf = get_test_config(&ProxyMode::Isolated).await;
        let result = send_frl_activation(&conf, &MockOutcome::Isolated, "ac1").await;
        assert_eq!(result, 502);
        let conf = conf.clone_with_mode(&ProxyMode::Connected);
        let result = send_frl_activation(&conf, &MockOutcome::Success, "ac1").await;
        assert_eq!(result, 200);
        let conf = conf.clone_with_mode(&ProxyMode::Isolated);
        let result = send_frl_activation(&conf, &MockOutcome::Isolated, "ac1").await;
        assert_eq!(result, 200);
        release_test_config(conf).await;
    }

    #[tokio::test]
    async fn test_frl_deactivation_request() {
        let conf = get_test_config(&ProxyMode::Connected).await;
        let result = send_frl_deactivation(&conf, &MockOutcome::Success, "dr1").await;
        assert_eq!(result, 200);
        release_test_config(conf).await;
    }

    #[tokio::test]
    async fn test_frl_deactivation_cache() {
        let conf = get_test_config(&ProxyMode::Isolated).await;
        let result = send_frl_deactivation(&conf, &MockOutcome::Isolated, "dc1").await;
        assert_eq!(result, 502);
        let conf = conf.clone_with_mode(&ProxyMode::Connected);
        let result = send_frl_deactivation(&conf, &MockOutcome::Success, "dc1").await;
        assert_eq!(result, 200);
        let conf = conf.clone_with_mode(&ProxyMode::Isolated);
        let result = send_frl_deactivation(&conf, &MockOutcome::Isolated, "dc1").await;
        assert_eq!(result, 200);
        release_test_config(conf).await;
    }

    #[tokio::test]
    async fn test_frl_activation_deactivation_sequence() {
        let conf = get_test_config(&ProxyMode::Connected).await;
        let result = send_frl_activation(&conf, &MockOutcome::Success, "ads1").await;
        assert_eq!(result, 200);
        let result = send_frl_deactivation(&conf, &MockOutcome::Success, "ads1").await;
        assert_eq!(result, 200);
        let conf = conf.clone_with_mode(&ProxyMode::Isolated);
        let result = send_frl_activation(&conf, &MockOutcome::Isolated, "ads1").await;
        assert_eq!(result, 502);
        release_test_config(conf).await;
    }

    #[tokio::test]
    async fn test_nul_activation_request() {
        let conf = get_test_config(&ProxyMode::Connected).await;
        let result = send_nul_activation(&conf, &MockOutcome::Success, "nul1").await;
        assert_eq!(result, 200);
        release_test_config(conf).await;
    }

    #[tokio::test]
    async fn test_license_report() {
        let tempdir = get_test_directory().await;
        let conf = get_test_config(&ProxyMode::Connected).await;
        let result = send_nul_activation(&conf, &MockOutcome::Success, "nul1").await;
        assert_eq!(result, 200);
        let path = tempdir.join("launch-report1.csv");
        eprintln!("Launch report at: {:?}", path);
        conf.cache
            .report(&Datasource::Nul, path.to_str().unwrap(), false, false, false)
            .await
            .expect("Report failed");
        let content = std::fs::read_to_string(&path).expect("Can't read report");
        assert!(content.contains("MockApp1"));
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
        let tempdir = get_test_directory().await;
        let conf = get_test_config(&ProxyMode::Connected).await;
        let result = send_log_upload(&conf, &MockOutcome::Success, "lrr1").await;
        assert_eq!(result, 200);
        let path = tempdir.join("log-report1.csv");
        eprintln!("Log report at: {:?}", path);
        conf.cache
            .report(&Datasource::Log, path.to_str().unwrap(), false, false, false)
            .await
            .expect("Report failed");
        let content = std::fs::read_to_string(&path).expect("Can't read report");
        assert!(content.contains("lrr1"));
    }
}
