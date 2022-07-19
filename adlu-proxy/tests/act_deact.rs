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

fn construct_activation_request(
    builder: reqwest::blocking::RequestBuilder,
) -> reqwest::blocking::RequestBuilder {
    let activation_headers = vec![
        ("Accept-Encoding", "gzip, deflate, br"),
        ("X-Session-Id", "b9d54389-fdc4-4327-a773-1cafa696a531.1656461281312"),
        ("X-Api-Key", "ngl_photoshop1"),
        ("Content-Type", "application/json"),
        ("Accept", "application/json"),
        ("User-Agent", "NGL Client/1.30.0.1 (MAC/12.4.0) [2022-06-28T17:08:01.895-0700]"),
        ("X-Request-Id", "Req-Id-24b5dd41-f668-4d42-a27a-ec49ee7c731b"),
        ("Accept-Language", "en-us"),
    ];
    let activation_body = r#"
        {
            "appDetails" :
            {
                "currentAsnpId" : "",
                "nglAppId" : "Photoshop1",
                "nglAppVersion" : "23.4.1",
                "nglLibVersion" : "1.30.0.1"
            },
            "asnpTemplateId" : "WXpRNVptSXdPVFl0TkRjME55MDBNR001TFdKaE5HUXRNekZoWmpGaU9ERXpNR1V6e302Y2JjYTViYy01NTZjLTRhNTYtYjgwNy05ZjNjMWFhM2VhZjc",
            "deviceDetails" :
            {
                "currentDate" : "2022-06-28T17:08:01.736-0700",
                "deviceId" : "2c93c8798aa2b6253c651e6efd5fe4694595a8dad82dc3d35de233df5928c2fa",
                "enableVdiMarkerExists" : false,
                "isOsUserAccountInDomain" : false,
                "isVirtualEnvironment" : false,
                "osName" : "MAC",
                "osUserId" : "b693be356ac52411389a6c06eede8b4e47e583818384cddc62aff78c3ece084d",
                "osVersion" : "12.4.0"
            },
            "npdId" : "YzQ5ZmIwOTYtNDc0Ny00MGM5LWJhNGQtMzFhZjFiODEzMGUz",
            "npdPrecedence" : 80
        }"#;
    let mut builder = builder;
    for (key, val) in activation_headers {
        builder = builder.header(key, val);
    }
    builder.body(activation_body)
}

#[test]
fn test_activation_request() {
    let config_dir = format!("{}/../rsrc/configurations", env!("CARGO_MANIFEST_DIR"));
    std::env::set_current_dir(config_dir).expect("Can't change directory");
    let executable = env!("CARGO_BIN_EXE_adlu-proxy");
    let mut proxy = subprocess::Popen::create(
        &[executable, "-c", "proxy-http.toml", "start"],
        subprocess::PopenConfig { ..Default::default() },
    )
    .expect("Can't create proxy");
    proxy
        .wait_timeout(std::time::Duration::from_secs(2))
        .expect("Couldn't wait for proxy to start");
    let client = reqwest::blocking::Client::new();
    let proxy_url = "http://localhost:8080/asnp/frl_connected/values/v2";
    let req =
        construct_activation_request(client.request(reqwest::Method::POST, proxy_url))
            .build()
            .expect("Failed to build request");
    let response = client.execute(req).expect("Request failed");
    assert_eq!(response.status(), http::StatusCode::OK);
    println!("Received activation response: {:?}", response);
    proxy.terminate().expect("Couldn't kill proxy");
}
