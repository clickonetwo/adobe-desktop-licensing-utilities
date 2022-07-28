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
pub mod api;
pub mod cache;
pub mod cli;
pub mod handlers;
pub mod logging;
pub mod proxy;
pub mod settings;
#[cfg(test)]
pub mod test_generators;

#[cfg(test)]
mod tests {
    use super::api;
    use super::cache::Cache;
    use super::settings::{ProxyConfiguration, Settings, SettingsRef};
    use super::test_generators as tg;

    #[tokio::test]
    async fn test_activation_request() {
        let config_dir = format!("{}/../rsrc/configurations", env!("CARGO_MANIFEST_DIR"));
        std::env::set_current_dir(config_dir).expect("Can't change directory");
        let settings = Settings::new(SettingsRef::test_config());
        let cache = Cache::from(&settings, true).await.unwrap();
        let conf = ProxyConfiguration::new(&settings, &cache).unwrap();
        let filter = api::activate_route(conf);
        let mut builder = warp::test::request();
        builder = builder.method("POST").path("/asnp/frl_connected/values/v2");
        builder = tg::mock_activation_request(
            "test_request_1",
            &tg::MockOutcome::Success,
            builder,
        );
        let response = builder.reply(&filter).await;
        eprintln!("response: {:?}", response);
        assert_eq!(response.status().as_u16(), 200);
    }
}
