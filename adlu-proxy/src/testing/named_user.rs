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
use adlu_parse::protocol::{NulActivationRequestBody, NulActivationResponseBody};

use super::{MockInfo, MockOutcome, MockRequestType};

pub fn mock_activation_request(
    ask: &MockOutcome,
    device_id: &str,
    builder: warp::test::RequestBuilder,
) -> warp::test::RequestBuilder {
    let mi = MockInfo::with_type_and_outcome(&MockRequestType::NulActivation, ask);
    let body = if matches!(ask, MockOutcome::FromAdobe) {
        NulActivationRequestBody::valid_from_device_id(device_id)
    } else {
        NulActivationRequestBody::mock_from_device_id(device_id)
    };
    let mut builder = builder.method("POST").path("/asnp/nud/v4");
    builder = builder
        .header("X-Request-Id", &mi.request_id())
        .header("X-Session-Id", &mi.session_id())
        .header("X-Api-Key", &mi.api_key());
    builder.json(&body)
}

pub fn mock_activation_response(req: reqwest::Request) -> reqwest::Response {
    let request_body = req.body().unwrap().as_bytes().unwrap();
    let request_data: NulActivationRequestBody =
        serde_json::from_slice(request_body).unwrap();
    let device_id = request_data.device_details.device_id.as_str();
    let body = NulActivationResponseBody::mock_from_device_id(device_id);
    let mut builder = http::Response::builder()
        .status(200)
        .header("Content-Type", "application/json;encoding=utf-8");
    builder = match req.headers().get("X-Request-Id") {
        None => builder,
        Some(val) => builder.header("X-Request-Id", val),
    };
    builder.body(body.to_body_string()).unwrap().into()
}
