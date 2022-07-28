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
use eyre::{eyre, Result, WrapErr};
use http::HeaderValue;
use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum MockOutcome {
    Success,
    StoreMode,
    NetworkError,
    ParseFailure,
    ErrorStatus,
}

impl Default for MockOutcome {
    fn default() -> Self {
        MockOutcome::Success
    }
}

#[derive(Debug, Clone, Default)]
pub struct MockInfo {
    pub request_name: String,
    pub request_id: String,
    pub session_id: String,
    pub outcome: MockOutcome,
}

lazy_static::lazy_static! {
    static ref MOCK_INFO_MAP: RwLock<HashMap<String, MockInfo>> = RwLock::new(HashMap::new());
}

impl MockInfo {
    pub fn new(name: &str) -> Self {
        Self::new_with_outcome(name, &MockOutcome::Success)
    }

    pub fn new_with_outcome(name: &str, outcome: &MockOutcome) -> Self {
        let uuid = Uuid::new_v4().hyphenated().to_string();
        let request_id = uuid.clone();
        let session_id = format!("{}.{}", uuid, chrono::Local::now().timestamp_millis());
        let mi = MockInfo {
            request_name: name.to_string(),
            request_id,
            session_id,
            outcome: outcome.clone(),
        };
        let mut map = MOCK_INFO_MAP.write().unwrap();
        map.borrow_mut().insert(mi.request_id.clone(), mi.clone());
        mi
    }
}

impl From<Option<&HeaderValue>> for MockInfo {
    fn from(hdr: Option<&HeaderValue>) -> Self {
        if let Some(hdr) = hdr {
            if let Ok(val) = hdr.to_str() {
                let map = MOCK_INFO_MAP.read().unwrap();
                return match map.get(val) {
                    Some(mi) => mi.clone(),
                    None => MockInfo::default(),
                };
            }
        }
        MockInfo::default()
    }
}

pub fn mock_activation_request(
    name: &str,
    ask: &MockOutcome,
    builder: warp::test::RequestBuilder,
) -> warp::test::RequestBuilder {
    let headers = vec![
        ("Accept-Encoding", "gzip, deflate, br"),
        ("X-Api-Key", "ngl_photoshop1"),
        ("Content-Type", "application/json"),
        ("Accept", "application/json"),
        ("User-Agent", "NGL Client/1.30.0.1 (MAC/12.4.0) [2022-06-28T17:08:01.895-0700]"),
        ("Accept-Language", "en-us"),
    ];
    let body = r#"
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
    let mi = MockInfo::new_with_outcome(name, ask);
    let mut builder = builder
        .header("X-Request-Id", &mi.request_id)
        .header("X-Session-Id", &mi.session_id);
    for (key, val) in headers {
        builder = builder.header(key, val)
    }
    builder.body(body)
}

pub fn mock_error_response(req: reqwest::Request) -> Result<reqwest::Response> {
    let body = r#"{"error": "Error response requested"}"#.as_bytes();
    let mut builder = http::Response::builder()
        .status(400)
        .header("Content-Type", "application/json;encoding=utf-8");
    for header_name in ["X-Request-Id", "X-Request-Name"] {
        builder = match req.headers().get(header_name) {
            None => builder,
            Some(val) => builder.header(header_name, val),
        };
    }
    let resp = builder.body(body).wrap_err("Can't build mock response")?;
    Ok(resp.into())
}

pub fn mock_invalid_body_response(req: reqwest::Request) -> Result<reqwest::Response> {
    let body = r#"{"invalid key": "invalid body"}"#.as_bytes();
    let mut builder = http::Response::builder()
        .status(200)
        .header("Content-Type", "application/json;encoding=utf-8");
    builder = match req.headers().get("X-Request-Id") {
        None => builder,
        Some(val) => builder.header("X-Request-Id", val),
    };
    let resp = builder.body(body).wrap_err("Can't build mock response")?;
    Ok(resp.into())
}

pub fn mock_activation_response(req: reqwest::Request) -> Result<reqwest::Response> {
    let body = r#"{
        "adobeCertSignedValues": {
            "signatures": {
                "signature1": "laj2sLb...elided...Oi9zqEy12olv6M",
                "signature2": "aSAqFfd...elided...XkbpwFzAWgoLQ"
            },
            "values": {
                "licenseExpiryTimestamp": "1750060801000",
                "enigmaData": "...elided...",
                "graceTime": "8553600000",
                "createdForVdi": "false",
                "profileStatus": "PROFILE_AVAILABLE",
                "effectiveEndTimestamp": "1741507201000",
                "licenseExpiryWarningStartTimestamp": "1749456001000",
                "nglLibRefreshInterval": "86400000",
                "licenseId": "8A935605037F4F02B7BA",
                "licensedFeatures": "...elided...",
                "appRefreshInterval": "86400000",
                "appEntitlementStatus": "SUBSCRIPTION"
            }
        },
        "customerCertSignedValues": {
            "signatures": {
                "customerSignature2": "LV5a3B2I...elided...jtolQDSI",
                "customerSignature1": "mmzlAlEc...elided...PXZI3oYY"
            },
            "values": "eyJucGRJZCI6Ill6UTVabUl3T1RZdE5EYzBOeTAwTUdNNUxXSmhOR1F0TXpGaFpqRmlPREV6TUdVeiIsImFzbnBJZCI6IjIyMWJmYWQ1LTBhZTMtNDY4MC05Mjc1LWY3ZDVjYTFjMjNmZiIsImNyZWF0aW9uVGltZXN0YW1wIjoxNjU2NDYxMjgyMDA5LCJjYWNoZUxpZmV0aW1lIjo5MzU5OTUxODk5MSwicmVzcG9uc2VUeXBlIjoiRlJMX0lOSVRJQUwiLCJjYWNoZUV4cGlyeVdhcm5pbmdDb250cm9sIjp7Indhcm5pbmdTdGFydFRpbWVzdGFtcCI6MTc0OTQ1NjAwMTAwMCwid2FybmluZ0ludGVydmFsIjo4NjQwMDAwMH0sInByZXZpb3VzQXNucElkIjoiIiwiZGV2aWNlSWQiOiIyYzkzYzg3OThhYTJiNjI1M2M2NTFlNmVmZDVmZTQ2OTQ1OTVhOGRhZDgyZGMzZDM1ZGUyMzNkZjU5MjhjMmZhIiwib3NVc2VySWQiOiJiNjkzYmUzNTZhYzUyNDExMzg5YTZjMDZlZWRlOGI0ZTQ3ZTU4MzgxODM4NGNkZGM2MmFmZjc4YzNlY2UwODRkIiwiZGV2aWNlRGF0ZSI6IjIwMjItMDYtMjhUMTc6MDg6MDEuNzM2LTA3MDAiLCJzZXNzaW9uSWQiOiJiOWQ1NDM4OS1mZGM0LTQzMjctYTc3My0xY2FmYTY5NmE1MzEuMTY1NjQ2MTI4MTMxMi9TVUJTRVFVRU5UIn0"
        }
    }"#.as_bytes();
    let mut builder = http::Response::builder()
        .status(200)
        .header("Content-Type", "application/json;encoding=utf-8");
    builder = match req.headers().get("X-Request-Id") {
        None => builder,
        Some(val) => builder.header("X-Request-Id", val),
    };
    let resp = builder.body(body).wrap_err("Can't build mock response")?;
    Ok(resp.into())
}

pub fn mock_deactivation_response(req: reqwest::Request) -> Result<reqwest::Response> {
    let body = r#"{"invalidationSuccessful": true}"#.as_bytes();
    let mut builder = http::Response::builder()
        .status(200)
        .header("Content-Type", "application/json;encoding=utf-8");
    builder = match req.headers().get("X-Request-Id") {
        None => builder,
        Some(val) => builder.header("X-Request-Id", val),
    };
    let resp = builder.body(body).wrap_err("Can't build mock response")?;
    Ok(resp.into())
}

pub async fn mock_adobe_server(req: reqwest::Request) -> Result<reqwest::Response> {
    let mi: MockInfo = req.headers().get("X-Request-Id").into();
    match mi.outcome {
        MockOutcome::Success => {
            if req.method() == "POST" {
                mock_activation_response(req)
            } else {
                mock_deactivation_response(req)
            }
        }
        MockOutcome::StoreMode => panic!("request sent in StoreMode"),
        MockOutcome::NetworkError => Err(eyre!("NetworkError - server not reachable")),
        MockOutcome::ParseFailure => mock_invalid_body_response(req),
        MockOutcome::ErrorStatus => mock_error_response(req),
    }
}
