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
use adlu_proxy::cache::Cache;
use adlu_proxy::settings::{Settings, SettingsRef};
use adlu_proxy::test_generators as tg;
use adlu_proxy::{api, cli, handlers, settings};

#[tokio::test]
async fn test_activation_request() {
    let config_dir = format!("{}/../rsrc/configurations", env!("CARGO_MANIFEST_DIR"));
    std::env::set_current_dir(config_dir).expect("Can't change directory");
    let settings = Settings::new(SettingsRef::test_config());
    let cache = Cache::from(&settings, true).await.unwrap();
    let conf = settings::ProxyConfiguration::new(&settings, &cache).unwrap();
    let filter = api::activate_route(conf);
    let builder =
        warp::test::request().method("POST").path("/asnp/frl_connected/values/v2");
    let response =
        tg::mock_activation_request("test_request_1", tg::MockOutcome::Success, builder)
            .reply(&filter)
            .await;
    assert_eq!(response.status().as_u16(), 200);
}
