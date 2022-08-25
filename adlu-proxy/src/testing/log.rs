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

pub fn mock_log_upload_request(
    ask: &MockOutcome,
    builder: warp::test::RequestBuilder,
) -> warp::test::RequestBuilder {
    let headers = vec![
        ("Accept-Encoding", "gzip, deflate, br"),
        ("X-Api-Key", "ngl_illustrator1"),
        ("Content-Type", "text/plain; charset=utf-8"),
        ("Accept", "*/*"),
        ("User-Agent", "Illustrator/26.4.1 CFNetwork/1335.0.3 Darwin/21.6.0"),
        ("Accept-Language", "en-us"),
    ];
    let body = r##"SessionID=8be2322f-1774-47c7-88ef-b71cfc5a3fb8.1660322760908 Timestamp=2022-08-12T09:51:03:985-0700 ThreadID=4466787 Component=ngl-lib_HTTPRequestMac Description=\"SendHttpRequestSyncInternal: Calling endpoint https://cc-api-data.adobe.io/ingest\"
SessionID=8be2322f-1774-47c7-88ef-b71cfc5a3fb8.1660322760908 Timestamp=2022-08-12T09:51:04:244-0700 ThreadID=4468005 Component=ngl-lib_HttpRequestDelegate Description=\"Received Response with status 200\"
SessionID=8be2322f-1774-47c7-88ef-b71cfc5a3fb8.1660322760908 Timestamp=2022-08-12T09:51:04:244-0700 ThreadID=4467217 Component=ngl-lib_HttpRequestDelegate Description=\"Got empty response\"
SessionID=8be2322f-1774-47c7-88ef-b71cfc5a3fb8.1660322760908 Timestamp=2022-08-12T09:51:04:244-0700 ThreadID=4466787 Component=ngl-lib_HttpRestConnectorForPost Description=\"SendHttpRequestSync: Request complete\"
SessionID=8be2322f-1774-47c7-88ef-b71cfc5a3fb8.1660322760908 Timestamp=2022-08-12T09:56:01:627-0700 ThreadID=4466789 Component=ngl-lib_NglCommonLib Description=\"ImsCachedAccessToken - Cached access token fetch status: IMSConnectorStatus:0\"
SessionID=8be2322f-1774-47c7-88ef-b71cfc5a3fb8.1660322760908 Timestamp=2022-08-12T09:56:01:630-0700 ThreadID=4466789 Component=ngl-lib_HttpRestConnectorForPost Description=\"SendHttpRequestSync: Request about to start\"
SessionID=8be2322f-1774-47c7-88ef-b71cfc5a3fb8.1660322760908 Timestamp=2022-08-12T09:56:01:630-0700 ThreadID=4466789 Component=ngl-lib_HTTPRequestMac Description=\"SendHttpRequestSyncInternal: Calling endpoint https://lcs-ulecs.adobe.io/ulecs/v1\"
SessionID=8be2322f-1774-47c7-88ef-b71cfc5a3fb8.1660322760908 Timestamp=2022-08-12T09:56:01:996-0700 ThreadID=4468007 Component=ngl-lib_HttpRequestDelegate Description=\"Received Response with status 200\"
SessionID=8be2322f-1774-47c7-88ef-b71cfc5a3fb8.1660322760908 Timestamp=2022-08-12T09:56:01:996-0700 ThreadID=4468007 Component=ngl-lib_HttpRequestDelegate Description=\"Got empty response\"
SessionID=8be2322f-1774-47c7-88ef-b71cfc5a3fb8.1660322760908 Timestamp=2022-08-12T09:56:01:996-0700 ThreadID=4466789 Component=ngl-lib_HttpRestConnectorForPost Description=\"SendHttpRequestSync: Request complete\"
SessionID=8be2322f-1774-47c7-88ef-b71cfc5a3fb8.1660322760908 Timestamp=2022-08-12T10:01:00:953-0700 ThreadID=4466787 Component=ngl-lib_HttpRestConnectorForPost Description=\"SendHttpRequestSync: Request about to start\"
"##;
    let mi = MockInfo::with_type_and_outcome(&MockRequestType::LogUpload, ask);
    let mut builder = builder.method("POST").path("/ulecs/v1");
    builder = builder.header("Authorization", mi.authorization());
    for (key, val) in headers {
        builder = builder.header(key, val)
    }
    builder.body(body)
}

pub fn mock_log_response(_req: reqwest::Request) -> reqwest::Response {
    http::Response::builder().status(200).body("").unwrap().into()
}
