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
use super::{MockInfo, MockOutcome, MockRequestType};
use adlu_parse::protocol::LogSession;

pub fn mock_log_upload_request(
    ask: &MockOutcome,
    session_id: &str,
    builder: warp::test::RequestBuilder,
) -> warp::test::RequestBuilder {
    let session = LogSession::mock_from_session_id(session_id);
    let mi = MockInfo::with_type_and_outcome(&MockRequestType::LogUpload, ask);
    let mut builder = builder.method("POST").path("/ulecs/v1");
    builder = builder
        .header("Authorization", &mi.authorization())
        .header("X-Api-Key", &mi.api_key());
    builder.body(session.to_body())
}

pub fn mock_log_response(_req: reqwest::Request) -> reqwest::Response {
    http::Response::builder().status(200).body("").unwrap().into()
}
