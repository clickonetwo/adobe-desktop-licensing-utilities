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
use std::collections::HashMap;

use eyre::{eyre, Result, WrapErr};
use uuid::Uuid;

pub use frl::{mock_activation_request, mock_deactivation_request};

use super::settings::{LogLevel, ProxyMode, Settings, SettingsVal};
use super::{cache, logging, proxy, settings};

pub use self::log::mock_log_upload_request;

mod frl;
mod log;

#[derive(Default)]
struct SharedCache {
    count: usize,
    log_initialized: bool,
    cache: Option<cache::Cache>,
}

lazy_static::lazy_static! {
    static ref SHARED_CACHE: tokio::sync::RwLock<SharedCache> = tokio::sync::RwLock::new(Default::default());
}

async fn init_logging_and_cache() -> cache::Cache {
    let mut shared_cache = SHARED_CACHE.write().await;
    if shared_cache.count == 0 {
        // initialize logging and create the cache
        let tempdir = get_test_directory().await;
        if !shared_cache.log_initialized {
            let logging = settings::Logging {
                level: LogLevel::Debug,
                destination: settings::LogDestination::File,
                file_path: tempdir.join("proxy-log.log").to_str().unwrap().to_string(),
                ..Default::default()
            };
            logging::init(&logging).unwrap();
            shared_cache.log_initialized = true;
        }
        let path = tempdir.join("proxy-cache.sqlite").to_str().unwrap().to_string();
        let cache = cache::connect(&path).await.expect("Cache initialization failed");
        cache.clear(true).await.expect("Cache clear failed");
        shared_cache.cache = Some(cache);
        shared_cache.count = 1;
    } else {
        // increase the refcount on the existing cache
        shared_cache.count += 1;
    }
    shared_cache.cache.clone().unwrap()
}

async fn release_cache() {
    let mut shared_cache = SHARED_CACHE.write().await;
    if shared_cache.count == 0 {
        panic!("Tried to release cache before creating it!");
    }
    shared_cache.count -= 1;
    if shared_cache.count == 0 {
        // release the cache
        shared_cache.cache.as_ref().unwrap().close().await;
        shared_cache.cache = None;
    }
}

#[derive(Debug, Default)]
struct Tempdir {
    exists: bool,
    path: std::path::PathBuf,
}

lazy_static::lazy_static! {
    static ref SHARED_DIRECTORY: tokio::sync::RwLock<Tempdir> = tokio::sync::RwLock::new(Default::default());
}

pub async fn get_test_directory() -> std::path::PathBuf {
    // this is a critical section, because it interacts with the file system
    let mut tempdir = SHARED_DIRECTORY.write().await;
    if tempdir.exists {
        tempdir.path.clone()
    } else {
        let path = std::env::temp_dir().join("adlu-proxy-test");
        if std::fs::metadata(&path).is_err() {
            std::fs::create_dir(&path).expect("Test directory couldn't be created");
        }
        tempdir.path = path;
        tempdir.exists = true;
        tempdir.path.clone()
    }
}

pub async fn get_test_config(mode: &ProxyMode) -> proxy::Config {
    let cache = init_logging_and_cache().await;
    let mut settings = SettingsVal::default_config();
    settings.proxy.mode = mode.clone();
    let settings = Settings::new(settings);
    proxy::Config::new(settings, cache).unwrap()
}

pub async fn release_test_config(_config: proxy::Config) {
    release_cache().await;
}

#[derive(Debug, Clone)]
pub enum MockOutcome {
    Success,
    Isolated,
    Unreachable,
    ParseFailure,
    ErrorStatus,
    FromAdobe,
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
    static ref MOCK_INFO_MAP: std::sync::RwLock<HashMap<String, MockInfo>> = std::sync::RwLock::new(HashMap::new());
}

impl MockInfo {
    pub fn with_type_and_outcome(rtype: &MockRequestType, outcome: &MockOutcome) -> Self {
        let rtype = rtype.clone();
        let uuid = Uuid::new_v4().hyphenated().to_string();
        let outcome = outcome.clone();
        let mi = MockInfo { rtype, uuid, outcome };
        let mut map = MOCK_INFO_MAP.write().unwrap();
        map.insert(mi.uuid.clone(), mi.clone());
        mi
    }

    pub fn request_id(&self) -> String {
        format!("Req-Id-{}", &self.uuid)
    }

    pub fn session_id(&self) -> String {
        format!("{}.{}", self.uuid, chrono::Local::now().timestamp_millis())
    }

    pub fn authorization(&self) -> String {
        self.uuid.clone()
    }

    pub fn api_key(&self) -> String {
        if matches!(self.outcome, MockOutcome::FromAdobe) {
            if matches!(self.rtype, MockRequestType::Deactivation) {
                "adobe_licensing_toolkit".to_string()
            } else {
                "ngl_photoshop1".to_string()
            }
        } else {
            "ngl_mock1".to_string()
        }
    }
}

impl From<&reqwest::Request> for MockInfo {
    fn from(req: &reqwest::Request) -> Self {
        let headers = req.headers();
        let val = if let Some(hdr) = headers.get("X-Request-Id") {
            hdr.to_str().unwrap()[7..].to_string()
        } else {
            headers
                .get("Authorization")
                .expect("No Authorization header")
                .to_str()
                .unwrap()
                .to_string()
        };
        let map = MOCK_INFO_MAP.read().unwrap();
        map.get(val.as_str()).unwrap().clone()
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

pub async fn mock_adobe_server(
    conf: &proxy::Config,
    req: reqwest::Request,
) -> Result<reqwest::Response> {
    let mi: MockInfo = (&req).into();
    match mi.outcome {
        MockOutcome::Success => match mi.rtype {
            MockRequestType::Activation => Ok(frl::mock_activation_response(req)),
            MockRequestType::Deactivation => Ok(frl::mock_deactivation_response(req)),
            MockRequestType::LogUpload => Ok(log::mock_log_response(req)),
        },
        MockOutcome::Isolated => panic!("request sent in Isolated mode"),
        MockOutcome::Unreachable => {
            Err(eyre!("NetworkError - server not reachable requested"))
        }
        MockOutcome::ParseFailure => mock_parse_failure_response(req),
        MockOutcome::ErrorStatus => mock_error_status_response(req),
        MockOutcome::FromAdobe => {
            let result = conf.client.execute(req).await;
            result.wrap_err("Network error sending request to Adobe")
        }
    }
}
