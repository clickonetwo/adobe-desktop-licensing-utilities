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
use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::sync::RwLock;

use eyre::{eyre, Result, WrapErr};
use uuid::Uuid;

pub use frl::{mock_activation_request, mock_deactivation_request};

use crate::settings::{ProxyMode, SettingsVal};
use crate::Settings;

use super::{cache, logging, proxy, settings};

pub use self::log::mock_log_upload_request;

mod frl;
mod log;

pub async fn test_config(name: &str) -> proxy::Config {
    test_config_with_mode(name, &ProxyMode::Connected).await
}

pub async fn test_config_with_mode(name: &str, mode: &ProxyMode) -> proxy::Config {
    let mut settings = SettingsVal::default_config();
    settings.proxy.mode = mode.clone();
    let tempdir = std::env::temp_dir().join(name);
    if std::fs::metadata(&tempdir).is_err() {
        std::fs::create_dir(&tempdir).expect("Test directory couldn't be created");
    }
    settings.proxy.db_path =
        tempdir.join("proxy-cache.sqlite").to_str().unwrap().to_string();
    settings.proxy.host = "127.0.0.1".to_string();
    settings.logging.file_path =
        tempdir.join("proxy-log.log").to_str().unwrap().to_string();
    settings.logging.level = settings::LogLevel::Debug;
    settings.logging.destination = settings::LogDestination::File;
    let settings = Settings::new(settings);
    logging::init(&settings).unwrap();
    let cache = cache::connect(&settings).await.unwrap();
    proxy::Config::new(settings, cache).unwrap()
}

#[derive(Debug, Clone)]
pub enum MockOutcome {
    Success,
    Isolated,
    Uncreachable,
    ParseFailure,
    ErrorStatus,
}

#[derive(Debug, Clone)]
enum MockRequestType {
    Activation,
    Deactivation,
    LogUpload,
}

#[derive(Debug, Clone)]
struct MockInfo {
    rtype: MockRequestType,
    uuid: String,
    outcome: MockOutcome,
}

lazy_static::lazy_static! {
    static ref MOCK_INFO_MAP: RwLock<HashMap<String, MockInfo>> = RwLock::new(HashMap::new());
}

impl MockInfo {
    pub fn with_type_and_outcome(rtype: &MockRequestType, outcome: &MockOutcome) -> Self {
        let rtype = rtype.clone();
        let uuid = Uuid::new_v4().hyphenated().to_string();
        let outcome = outcome.clone();
        let mi = MockInfo { rtype, uuid, outcome };
        let mut map = MOCK_INFO_MAP.write().unwrap();
        map.borrow_mut().insert(mi.uuid.clone(), mi.clone());
        mi
    }

    pub fn request_id(&self) -> String {
        self.uuid.clone()
    }

    pub fn session_id(&self) -> String {
        format!("{}.{}", self.uuid, chrono::Local::now().timestamp_millis())
    }

    pub fn authorization(&self) -> String {
        self.uuid.clone()
    }
}

impl From<&reqwest::Request> for MockInfo {
    fn from(req: &reqwest::Request) -> Self {
        let headers = req.headers();
        let val = if let Some(hdr) = headers.get("X-Request-Id") {
            hdr.to_str().unwrap()
        } else {
            headers
                .get("Authorization")
                .expect("No Authorization header")
                .to_str()
                .unwrap()
        };
        let map = MOCK_INFO_MAP.read().unwrap();
        map.get(val).unwrap().clone()
    }
}

pub fn mock_error_status_response(req: reqwest::Request) -> Result<reqwest::Response> {
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

pub fn mock_parse_failure_response(req: reqwest::Request) -> Result<reqwest::Response> {
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

pub async fn mock_adobe_server(req: reqwest::Request) -> Result<reqwest::Response> {
    let mi: MockInfo = (&req).into();
    match mi.outcome {
        MockOutcome::Success => match mi.rtype {
            MockRequestType::Activation => Ok(frl::mock_activation_response(req)),
            MockRequestType::Deactivation => Ok(frl::mock_deactivation_response(req)),
            MockRequestType::LogUpload => Ok(log::mock_log_response(req)),
        },
        MockOutcome::Isolated => panic!("request sent in StoreMode"),
        MockOutcome::Uncreachable => {
            Err(eyre!("NetworkError - server not reachable requested"))
        }
        MockOutcome::ParseFailure => mock_parse_failure_response(req),
        MockOutcome::ErrorStatus => mock_error_status_response(req),
    }
}
