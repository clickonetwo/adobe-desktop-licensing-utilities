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
use lazy_static::lazy_static;
use regex::bytes::Regex;
use serde::{Deserialize, Serialize};
use warp::{Filter, Reply};

use adlu_base::Timestamp;

use crate::{AdobeSignatures, CustomerSignatures};

/// There are two kinds of requests and responses: activation
/// and deactivation.  But pretty much all the actions you
/// take with both kinds are the same: cache them, send the
/// requests to server, get the responses from the server.
/// Also, there are times when we need to process collections
/// of requests that are ordered by timestamp, so having
/// an umbrella type that can be carried in a collection
/// allows sorting that collection by timestamp.
#[derive(Debug, Clone)]
pub enum Request {
    Activation(Box<FrlActivationRequest>),
    Deactivation(Box<FrlDeactivationRequest>),
    LogUpload(Box<LogUploadRequest>),
}

impl Request {
    pub fn timestamp(&self) -> &Timestamp {
        match self {
            Request::Activation(req) => &req.timestamp,
            Request::Deactivation(req) => &req.timestamp,
            Request::LogUpload(req) => &req.timestamp,
        }
    }

    pub fn request_id(&self) -> &str {
        match self {
            Request::Activation(req) => &req.request_id,
            Request::Deactivation(req) => &req.request_id,
            Request::LogUpload(req) => &req.request_id,
        }
    }

    /// a [`warp::Filter`] that produces an `FrlRequest` from a well-formed
    /// activation network request posted to a warp server.  You can compose this filter with
    /// a path filter to provide an activation hander at a specific endpoint.
    pub fn activation_filter(
    ) -> impl Filter<Extract = (Self,), Error = warp::Rejection> + Clone {
        warp::post()
            .and(warp::header::<String>("X-Session-Id"))
            .and(warp::header::<String>("X-Request-Id"))
            .and(warp::header::<String>("X-Api-Key"))
            .and(warp::body::json())
            .map(
                |session_id: String,
                 request_id: String,
                 api_key: String,
                 parsed_body: FrlActivationRequestBody| {
                    Request::Activation(Box::new(FrlActivationRequest {
                        timestamp: Timestamp::now(),
                        api_key,
                        request_id,
                        session_id,
                        parsed_body,
                    }))
                },
            )
    }

    /// a [`warp::Filter`] that produces an `FrlRequest` from a well-formed
    /// deactivation network request posted to a warp server.  You can compose this filter with
    /// a path filter to provide a deactivation hander at a specific endpoint.
    pub fn deactivation_filter(
    ) -> impl Filter<Extract = (Self,), Error = warp::Rejection> + Clone {
        warp::delete()
            .and(warp::header::<String>("X-Request-Id"))
            .and(warp::header::<String>("X-Api-Key"))
            .and(warp::query::<FrlDeactivationQueryParams>())
            .map(
                |request_id: String,
                 api_key: String,
                 params: FrlDeactivationQueryParams| {
                    Request::Deactivation(Box::new(FrlDeactivationRequest {
                        timestamp: Timestamp::now(),
                        api_key,
                        request_id,
                        params,
                    }))
                },
            )
    }

    /// a [`warp::Filter`] that produces a `LogUploadRequest` from a well-formed
    /// log upload network request posted to a warp server.  You can compose this filter with
    /// a log upload filter to provide a log upload hander at a specific endpoint.
    pub fn log_filter() -> impl Filter<Extract = (Self,), Error = warp::Rejection> + Clone
    {
        warp::post()
            .and(warp::header::<String>("Authorization"))
            .and(warp::header::<String>("X-Api-Key"))
            .and(warp::body::content_length_limit(11 * 1024 * 1024))
            .and(warp::body::bytes())
            .map(|authorization: String, api_key: String, log_data: bytes::Bytes| {
                let session_data = parse_log_data(log_data.clone());
                let timestamp = Timestamp::now();
                let request_id = format!("{}.{}", api_key, timestamp);
                Request::LogUpload(Box::new(LogUploadRequest {
                    timestamp,
                    request_id,
                    authorization,
                    api_key,
                    log_data,
                    session_data,
                }))
            })
    }

    pub fn to_network(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        match self {
            Request::Activation(req) => req.to_network(builder),
            Request::Deactivation(req) => req.to_network(builder),
            Request::LogUpload(req) => req.to_network(builder),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Response {
    Activation(Box<FrlActivationResponse>),
    Deactivation(Box<FrlDeactivationResponse>),
    LogUpload(Box<LogUploadResponse>),
}

impl Response {
    pub fn timestamp(&self) -> &Timestamp {
        match self {
            Response::Activation(resp) => &resp.timestamp,
            Response::Deactivation(resp) => &resp.timestamp,
            Response::LogUpload(resp) => &resp.timestamp,
        }
    }

    pub async fn from_network(req: &Request, resp: reqwest::Response) -> Result<Self> {
        match req {
            Request::Activation(_) => {
                let resp = FrlActivationResponse::from_network(resp).await?;
                Ok(Response::Activation(Box::new(resp)))
            }
            Request::Deactivation(_) => {
                let resp = FrlDeactivationResponse::from_network(resp).await?;
                Ok(Response::Deactivation(Box::new(resp)))
            }
            Request::LogUpload(_) => {
                let resp = LogUploadResponse::from_network(resp).await?;
                Ok(Response::LogUpload(Box::new(resp)))
            }
        }
    }
}

impl From<Response> for warp::reply::Response {
    fn from(resp: Response) -> Self {
        match resp {
            Response::Activation(resp) => resp.into_response(),
            Response::Deactivation(resp) => resp.into_response(),
            Response::LogUpload(resp) => resp.into_response(),
        }
    }
}

impl Reply for Response {
    fn into_response(self) -> warp::reply::Response {
        self.into()
    }
}

#[derive(Debug, Clone)]
pub struct FrlActivationRequest {
    pub timestamp: Timestamp,
    pub api_key: String,
    pub request_id: String,
    pub session_id: String,
    pub parsed_body: FrlActivationRequestBody,
}

impl FrlActivationRequest {
    pub fn activation_id(&self) -> String {
        let d_id = self.deactivation_id();
        let factors: Vec<&str> = vec![
            &self.parsed_body.app_details.ngl_app_id,
            &self.parsed_body.app_details.ngl_lib_version,
            &d_id,
        ];
        factors.join("|")
    }

    pub fn deactivation_id(&self) -> String {
        let factors: Vec<&str> = vec![
            &self.parsed_body.npd_id,
            if self.parsed_body.device_details.enable_vdi_marker_exists
                && self.parsed_body.device_details.is_virtual_environment
            {
                &self.parsed_body.device_details.os_user_id
            } else {
                &self.parsed_body.device_details.device_id
            },
        ];
        factors.join("|")
    }

    pub fn to_network(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        builder
            .header("X-Request-Id", &self.request_id)
            .header("X-Session-Id", &self.session_id)
            .header("X-Api-Key", &self.api_key)
            .json(&self.parsed_body)
    }
}

#[derive(Debug, Clone)]
pub struct FrlDeactivationRequest {
    pub timestamp: Timestamp,
    pub api_key: String,
    pub request_id: String,
    pub params: FrlDeactivationQueryParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrlDeactivationQueryParams {
    pub npd_id: String,
    pub device_id: String,
    pub os_user_id: String,
    pub enable_vdi_marker_exists: i8,
    pub is_virtual_environment: i8,
    pub is_os_user_account_in_domain: i8,
}

impl FrlDeactivationRequest {
    pub fn deactivation_id(&self) -> String {
        let factors: Vec<&str> = vec![
            &self.params.npd_id,
            if self.params.enable_vdi_marker_exists != 0
                && self.params.is_virtual_environment != 0
            {
                &self.params.os_user_id
            } else {
                &self.params.device_id
            },
        ];
        factors.join("|")
    }

    pub fn to_network(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        builder
            .header("X-Request-Id", &self.request_id)
            .header("X-Api-Key", &self.api_key)
            .query(&self.params)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrlActivationRequestBody {
    pub app_details: FrlAppDetails,
    pub asnp_template_id: String,
    pub device_details: FrlDeviceDetails,
    pub npd_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npd_precedence: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrlAppDetails {
    #[serde(default)]
    pub current_asnp_id: String,
    pub ngl_app_id: String,
    pub ngl_app_version: String,
    pub ngl_lib_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrlDeviceDetails {
    pub current_date: String,
    pub device_id: String,
    pub enable_vdi_marker_exists: bool,
    pub is_os_user_account_in_domain: bool,
    pub is_virtual_environment: bool,
    pub os_name: String,
    pub os_user_id: String,
    pub os_version: String,
}

#[derive(Debug, Clone)]
pub struct FrlActivationResponse {
    pub timestamp: Timestamp,
    pub request_id: String,
    pub body: String,
    pub parsed_body: Option<FrlActivationResponseBody>,
}

impl FrlActivationResponse {
    pub async fn from_network(response: reqwest::Response) -> Result<Self> {
        let request_id = match response.headers().get("X-Request-Id") {
            None => {
                return Err(eyre!("No activation request-id"));
            }
            Some(val) => {
                val.to_str().wrap_err("Invalid activation request-id")?.to_string()
            }
        };
        let body = response.text().await.wrap_err("Failure to receive body")?;
        let parsed_body: Option<FrlActivationResponseBody> =
            if cfg!(feature = "parse_responses") {
                Some(
                    serde_json::from_str::<FrlActivationResponseBody>(&body)
                        .wrap_err("Invalid activation response")?,
                )
            } else {
                None
            };
        Ok(FrlActivationResponse {
            timestamp: Timestamp::now(),
            request_id,
            body,
            parsed_body,
        })
    }
}

impl From<FrlActivationResponse> for warp::reply::Response {
    fn from(act_resp: FrlActivationResponse) -> Self {
        ::http::Response::builder()
            .header("X-Request-Id", &act_resp.request_id)
            .body(act_resp.body.into())
            .unwrap()
    }
}

impl Reply for FrlActivationResponse {
    fn into_response(self) -> warp::reply::Response {
        self.into()
    }
}

#[derive(Debug, Clone)]
pub struct FrlDeactivationResponse {
    pub timestamp: Timestamp,
    pub request_id: String,
    pub body: String,
    pub parsed_body: Option<FrlDeactivationResponseBody>,
}

impl FrlDeactivationResponse {
    pub async fn from_network(response: reqwest::Response) -> Result<Self> {
        let request_id = match response.headers().get("X-Request-Id") {
            None => {
                return Err(eyre!("No deactivation request-id"));
            }
            Some(val) => {
                val.to_str().wrap_err("Invalid deactivation request-id")?.to_string()
            }
        };
        let body = response.text().await.wrap_err("Failure to receive body")?;
        let parsed_body: Option<FrlDeactivationResponseBody> =
            if cfg!(feature = "parse_responses") {
                Some(
                    serde_json::from_str::<FrlDeactivationResponseBody>(&body)
                        .wrap_err("Invalid deactivation response")?,
                )
            } else {
                None
            };
        Ok(FrlDeactivationResponse {
            timestamp: Timestamp::now(),
            request_id,
            body,
            parsed_body,
        })
    }
}

impl From<FrlDeactivationResponse> for warp::reply::Response {
    fn from(act_resp: FrlDeactivationResponse) -> Self {
        ::http::Response::builder()
            .header("X-Request-Id", &act_resp.request_id)
            .body(act_resp.body.into())
            .unwrap()
    }
}

impl Reply for FrlDeactivationResponse {
    fn into_response(self) -> warp::reply::Response {
        self.into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrlActivationResponseBody {
    pub adobe_cert_signed_values: FrlAdobeCertSignedValues,
    pub customer_cert_signed_values: FrlCustomerCertSignedValues,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrlAdobeCertSignedValues {
    pub signatures: AdobeSignatures,
    pub values: FrlAdobeSignedValues,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrlAdobeSignedValues {
    pub license_expiry_timestamp: String,
    pub enigma_data: String,
    pub grace_time: String,
    pub created_for_vdi: String,
    pub profile_status: String,
    pub effective_end_timestamp: String,
    pub license_expiry_warning_start_timestamp: String,
    pub ngl_lib_refresh_interval: String,
    pub license_id: String,
    pub licensed_features: String,
    pub app_refresh_interval: String,
    pub app_entitlement_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrlCustomerCertSignedValues {
    pub signatures: CustomerSignatures,
    #[serde(deserialize_with = "adlu_base::base64_encoded_json::deserialize")]
    pub values: FrlCustomerSignedValues,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrlCustomerSignedValues {
    pub npd_id: String,
    pub asnp_id: String,
    pub creation_timestamp: i64,
    pub cache_lifetime: i64,
    pub response_type: String,
    pub cache_expiry_warning_control: CacheExpiryWarningControl,
    pub previous_asnp_id: String,
    pub device_id: String,
    pub os_user_id: String,
    pub device_date: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheExpiryWarningControl {
    warning_start_timestamp: i64,
    warning_interval: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrlDeactivationResponseBody {
    invalidation_successful: bool,
}

#[derive(Debug, Clone)]
pub struct LogUploadRequest {
    pub timestamp: Timestamp,
    pub request_id: String,
    pub authorization: String,
    pub api_key: String,
    pub log_data: bytes::Bytes,
    pub session_data: Vec<LogSession>,
}

impl LogUploadRequest {
    pub fn to_network(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        builder
            .header("Authorization", &self.authorization)
            .header("X-Api-Key", &self.api_key)
            .body(self.log_data.clone())
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSession {
    pub session_id: String,
    pub initial_entry: Timestamp,
    pub final_entry: Timestamp,
    pub session_start: Option<Timestamp>,
    pub session_end: Option<Timestamp>,
    pub app_id: Option<String>,
    pub app_version: Option<String>,
    pub app_locale: Option<String>,
    pub ngl_version: Option<String>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub user_id: Option<String>,
}

impl LogSession {
    pub fn merge(&self, other: &LogSession) -> Result<Self> {
        if self.session_id != other.session_id {
            Err(eyre!("Can't merge sessions with different IDs"))
        } else {
            Ok(LogSession {
                session_id: self.session_id.clone(),
                initial_entry: if self.initial_entry <= other.initial_entry {
                    self.initial_entry.clone()
                } else {
                    other.initial_entry.clone()
                },
                final_entry: if self.final_entry >= other.final_entry {
                    self.final_entry.clone()
                } else {
                    other.final_entry.clone()
                },
                session_start: if self.session_start.is_none() {
                    other.session_start.clone()
                } else {
                    self.session_start.clone()
                },
                session_end: if self.session_end.is_none() {
                    other.session_end.clone()
                } else {
                    self.session_end.clone()
                },
                app_id: if self.app_id.is_none() {
                    other.app_id.clone()
                } else {
                    self.app_id.clone()
                },
                app_version: if self.app_version.is_none() {
                    other.app_version.clone()
                } else {
                    self.app_version.clone()
                },
                app_locale: if self.app_locale.is_none() {
                    other.app_locale.clone()
                } else {
                    self.app_locale.clone()
                },
                ngl_version: if self.ngl_version.is_none() {
                    other.ngl_version.clone()
                } else {
                    self.ngl_version.clone()
                },
                os_name: if self.os_name.is_none() {
                    other.os_name.clone()
                } else {
                    self.os_name.clone()
                },
                os_version: if self.os_version.is_none() {
                    other.os_version.clone()
                } else {
                    self.os_version.clone()
                },
                user_id: if self.user_id.is_none() {
                    other.user_id.clone()
                } else {
                    self.user_id.clone()
                },
            })
        }
    }
}

lazy_static! {
    static ref RE_MAP: HashMap<&'static str, Regex> = {
        let mut map = HashMap::new();
        map.insert(
            "line",
            Regex::new(
                r#"(?m-u)^SessionID=([^ ]+) Timestamp=([^ ]+) .*Description="(.+)"\r?$"#,
            )
            .unwrap(),
        );
        map.insert("start", Regex::new(r"(?-u)Initializing session logs").unwrap());
        map.insert("end", Regex::new(r"(?-u)Terminating session logs").unwrap());
        map.insert(
            "os",
            Regex::new(r"(?-u)SetConfig:.+OS Name=([^,]+), OS Version=([^\s]+)").unwrap(),
        );
        map.insert(
            "app",
            Regex::new(r"(?-u)SetConfig:.+AppID=([^,]+), AppVersion=([^,]+)").unwrap(),
        );
        map.insert("ngl", Regex::new(r"(?-u)SetConfig:.+NGLLibVersion=([^,]+)").unwrap());
        map.insert(
            "locale",
            Regex::new(r"(?-u)SetAppRuntimeConfig:.+AppLocale=([a-zA-Z_]+)").unwrap(),
        );
        map.insert(
            "user",
            Regex::new(r"(?-u)LogCurrentUser:.+UserID=([a-z0-9]{40})").unwrap(),
        );
        map
    };
}

pub fn parse_log_data(data: bytes::Bytes) -> Vec<LogSession> {
    let line_pattern = &RE_MAP["line"];
    let mut sessions: Vec<LogSession> = Vec::new();
    let mut session: LogSession = Default::default();
    for cap in line_pattern.captures_iter(&data) {
        let sid = String::from_utf8(cap[1].to_vec()).unwrap();
        let time = String::from_utf8(cap[2].to_vec()).unwrap();
        let timestamp = Timestamp::from_storage(&time);
        if sid != session.session_id {
            if !session.session_id.is_empty() {
                sessions.push(session.clone())
            }
            session = LogSession {
                session_id: sid,
                initial_entry: timestamp.clone(),
                final_entry: timestamp.clone(),
                ..Default::default()
            }
        }
        parse_log_description(&mut session, &timestamp, &cap[3]);
    }
    if !session.session_id.is_empty() {
        sessions.push(session.clone())
    }
    sessions
}

fn parse_log_description(
    session: &mut LogSession,
    timestamp: &Timestamp,
    description: &[u8],
) {
    session.final_entry = timestamp.clone();
    if RE_MAP["start"].captures(description).is_some() {
        session.session_start = Some(timestamp.clone());
    } else if RE_MAP["end"].captures(description).is_some() {
        session.session_end = Some(timestamp.clone());
    } else if let Some(cap) = RE_MAP["os"].captures(description) {
        let os_name = String::from_utf8(cap[1].to_vec()).unwrap();
        let os_version = String::from_utf8(cap[2].to_vec()).unwrap();
        session.os_name = Some(os_name);
        session.os_version = Some(os_version);
    } else if let Some(cap) = RE_MAP["app"].captures(description) {
        let app_id = String::from_utf8(cap[1].to_vec()).unwrap();
        let app_version = String::from_utf8(cap[2].to_vec()).unwrap();
        session.app_id = Some(app_id);
        session.app_version = Some(app_version);
    } else if let Some(cap) = RE_MAP["ngl"].captures(description) {
        let ngl_version = String::from_utf8(cap[1].to_vec()).unwrap();
        session.ngl_version = Some(ngl_version);
    } else if let Some(cap) = RE_MAP["locale"].captures(description) {
        let locale = String::from_utf8(cap[1].to_vec()).unwrap();
        session.app_locale = Some(locale);
    } else if let Some(cap) = RE_MAP["user"].captures(description) {
        let user_id = String::from_utf8(cap[1].to_vec()).unwrap();
        session.user_id = Some(user_id);
    }
}

#[derive(Debug, Clone)]
pub struct LogUploadResponse {
    pub timestamp: Timestamp,
}

impl Default for LogUploadResponse {
    fn default() -> Self {
        LogUploadResponse { timestamp: Timestamp::now() }
    }
}

impl LogUploadResponse {
    pub fn new() -> Self {
        Default::default()
    }

    pub async fn from_network(_response: reqwest::Response) -> Result<Self> {
        Ok(Self::new())
    }
}

impl From<LogUploadResponse> for warp::reply::Response {
    fn from(_resp: LogUploadResponse) -> Self {
        warp::reply().into_response()
    }
}

impl Reply for LogUploadResponse {
    fn into_response(self) -> warp::reply::Response {
        self.into()
    }
}

#[cfg(test)]
mod test {
    use adlu_base::Timestamp;
    use std::fs::read_to_string;

    #[test]
    fn test_parse_activation_request() {
        let request_str = r#"
        {
            "appDetails" : 
            {
                "currentAsnpId" : "",
                "nglAppId" : "Photoshop1",
                "nglAppVersion" : "23.4.1",
                "nglLibVersion" : "1.30.0.1"
            },
            "asnpTemplateId" : "WXpRNVptS...elided...wNy05ZjNjMWFhM2VhZjc",
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
        let request: super::FrlActivationRequestBody =
            serde_json::from_str(request_str).unwrap();
        assert_eq!(request.app_details.ngl_app_id, "Photoshop1");
        assert!(!request.device_details.is_os_user_account_in_domain);
        assert_eq!(request.npd_precedence, Some(80));
    }

    #[test]
    fn test_parse_activation_response() {
        let response_str = r#"
        {
            "adobeCertSignedValues": {
                "signatures": {
                    "signature1": "laj2sLb...elided...Oi9zqEy12olv6M",
                    "signature2": "aSAqFfd...elided...XkbpwFzAWgoLQ"
                },
                "values": {
                    "licenseExpiryTimestamp": "1750060801000",
                    "enigmaData": "{\\\"productId\\\":45,\\\"serialKey\\\":\\\"104545012345537554907209\\\",\\\"clearSerialKey\\\":null,\\\"locale\\\":\\\"ALL\\\",\\\"associatedLocales\\\":\\\"ALL\\\",\\\"platform\\\":0,\\\"isk\\\":454042,\\\"customerId\\\":0,\\\"deliveryMethod\\\":3,\\\"pc\\\":true,\\\"rb\\\":false}",
                    "graceTime": "8553600000",
                    "createdForVdi": "false",
                    "profileStatus": "PROFILE_AVAILABLE",
                    "effectiveEndTimestamp": "1741507201000",
                    "licenseExpiryWarningStartTimestamp": "1749456001000",
                    "nglLibRefreshInterval": "86400000",
                    "licenseId": "8A935605037F4F02B7BA",
                    "licensedFeatures": "[\\\"AMT_SUBSCRIPTION_6.0\\\",\\\"Bridge_Base_4.0\\\",\\\"Bridge_Base_5.0\\\",\\\"Bridge_Base_6.0\\\",\\\"Bridge_CameraRaw_4.0\\\",\\\"Bridge_CameraRaw_5.0\\\",\\\"Bridge_CameraRaw_6.0\\\",\\\"Bridge_MiniBridge_1.0\\\",\\\"Bridge_MiniBridge_2.0\\\",\\\"Bridge_MiniBridge_3.0\\\",\\\"Euclid_Base_1.0\\\",\\\"MobileCenter_Base_3.0\\\",\\\"Photoshop_14.0\\\",\\\"Photoshop_Base_12.0\\\",\\\"Photoshop_Base_13.0\\\",\\\"Photoshop_Base_14.0\\\",\\\"Photoshop_Premium_12.0\\\",\\\"Photoshop_Premium_13.0\\\",\\\"Photoshop_Premium_14.0\\\"]",
                    "appRefreshInterval": "86400000",
                    "appEntitlementStatus": "SUBSCRIPTION"
                }
            },
            "customerCertSignedValues": {
                "signatures": {
                    "customerSignature2": "LV5a3B2I-1_Tr2Lev54_PntYTWKrevciaodE7lH933NpEeJrp1qBJPobXS0dKhSzUmiFyGeCCUat5SursCpce2291WcRIHOYAJjkMKfQ3Lr_L_gewGcnDvyCEbGHRluoqgXvGOJgD5gFJviDXAw754cIpoKhfnPCk9WtgfCQbnG6enmC6hWXCIgRx1dgAggC3HNW0Rh_13HdG71UCyYTYpMYXxZef-aM0UTvrVHEOM7NXyGu5mCnQZFIqhc--JCGdENPtUgEFBPqwWQfbMrdN31572iimU3FyIVy4L1yEQBHQFP3Ra9RX-_7gzmuo_wR9c0kBy7QpxM7UDYt-deqLb68i0txT3yFb_iWt0c55wORmOMcA3ZOelHPkZ6T01ey9-SeT-d2aTxTXzGIquu7eTBE_focGXEVfB11V4oQ3qG73uaxRfLQ_bkQYeh1vXLc9hvxswAtFb5aZ797XeJ3nb2uVGgr2AXFmeRQrowGEK4uK7DFUbrMwuZtj_1b6ZOh7n2Z8gIGQHlPkm3mqPLYt8mj7e1JCnc4_TcIccphWG6pkyZlq9Xuhdg8Yqg-IAW50qOP0uTAlXXYjLHt0SICIB6uPcnwG8frfAnP0X_vDkxf_uwaaPqBADcLWSOgu5i4KFbOPVayZqST1WbhJxlcw5odSbapwrfcRTTjtolQDSI",
                    "customerSignature1": "mmzlAlEcU6X_a-F77AaGRRrzAh-btGgRg_ymvttxR-fegm_8UxFL8qHJgZmsT4QvgqjuWOb5l05sn5kNeBcBAf-Tw1Gth_WOy9bWo8JS2fd3HBRyQSJkpsyhHPzBkqfEqUpNZkjm0tR73cS8aWtDTAkbF7BBNoEAVfIfvnPRId3tn_HMf04em26ED19MoYajBCyl-LuF25yqLutOh6-2By1A_ujajIFESL7jw2aTWXgf_eG7d6fMMliGn29FUvn-afEYUXO6Lhgfy9qrPtfPB4f7LNIWMTTKKtfxDotnQsF75Qri-A3OTeAe-tMXD55HIyJl-tZEXE14Dp9tapRhYA47MPYgGIVYch1OduGWPopqjAtzCn4423IugZRtQAzJWuT07qmBTBQHL_2cSWLve1UsSg9pjEroO9Grhayn6xxk44vIyoEz2g47JyZFRw0Oa8XxTN1e8FAlQZ6at-rh5F9afrenAb2ndmJxCG9NKI7_PmRy4U9v1BCxS34NT--S2gWIRKZHdc8wRIEZHy6MkF-80Y7KuRf9HpZOUR1Msxfap6bsnw9iHGE1akT8xMMZLOgRsWaLcab5jyhWVnLVeMTaFUtsRKrKQtnyvt6jUsaAz0EhyXigJ2R8XOmVsbdo8A86Adbfs_EXerQGGTHMuIVvJ5K1KHoHJ5dPXZI3oYY"
                },
                "values": "eyJucGRJZCI6Ill6UTVabUl3T1RZdE5EYzBOeTAwTUdNNUxXSmhOR1F0TXpGaFpqRmlPREV6TUdVeiIsImFzbnBJZCI6IjIyMWJmYWQ1LTBhZTMtNDY4MC05Mjc1LWY3ZDVjYTFjMjNmZiIsImNyZWF0aW9uVGltZXN0YW1wIjoxNjU2NDYxMjgyMDA5LCJjYWNoZUxpZmV0aW1lIjo5MzU5OTUxODk5MSwicmVzcG9uc2VUeXBlIjoiRlJMX0lOSVRJQUwiLCJjYWNoZUV4cGlyeVdhcm5pbmdDb250cm9sIjp7Indhcm5pbmdTdGFydFRpbWVzdGFtcCI6MTc0OTQ1NjAwMTAwMCwid2FybmluZ0ludGVydmFsIjo4NjQwMDAwMH0sInByZXZpb3VzQXNucElkIjoiIiwiZGV2aWNlSWQiOiIyYzkzYzg3OThhYTJiNjI1M2M2NTFlNmVmZDVmZTQ2OTQ1OTVhOGRhZDgyZGMzZDM1ZGUyMzNkZjU5MjhjMmZhIiwib3NVc2VySWQiOiJiNjkzYmUzNTZhYzUyNDExMzg5YTZjMDZlZWRlOGI0ZTQ3ZTU4MzgxODM4NGNkZGM2MmFmZjc4YzNlY2UwODRkIiwiZGV2aWNlRGF0ZSI6IjIwMjItMDYtMjhUMTc6MDg6MDEuNzM2LTA3MDAiLCJzZXNzaW9uSWQiOiJiOWQ1NDM4OS1mZGM0LTQzMjctYTc3My0xY2FmYTY5NmE1MzEuMTY1NjQ2MTI4MTMxMi9TVUJTRVFVRU5UIn0"
            }
        }
        "#;
        let response: super::FrlActivationResponseBody =
            serde_json::from_str(response_str).unwrap();
        assert_eq!(
            response.adobe_cert_signed_values.values.profile_status,
            "PROFILE_AVAILABLE"
        );
        assert_eq!(
            response.customer_cert_signed_values.values.npd_id,
            "YzQ5ZmIwOTYtNDc0Ny00MGM5LWJhNGQtMzFhZjFiODEzMGUz"
        )
    }

    #[test]
    fn test_parse_deactivation_response() {
        let response_str = r#"{"invalidationSuccessful":true}"#;
        let response: super::FrlDeactivationResponseBody =
            serde_json::from_str(response_str).unwrap();
        assert!(response.invalidation_successful);
    }

    #[test]
    fn test_parse_complete_log_upload() {
        let path = "../rsrc/logs/mac/NGLClient_PremierePro122.5.0.log.bin";
        let data = bytes::Bytes::from(read_to_string(path).unwrap());
        let sessions = super::parse_log_data(data);
        assert_eq!(sessions.len(), 1);
        let session = &sessions[0];
        let session_id = "4f7c3960-48da-49bb-9359-e0f040ecae66.1660326622129";
        let start = Timestamp::from_storage("2022-08-12T10:50:22:129-0700");
        let end = Timestamp::from_storage("2022-08-12T10:50:53:807-0700");
        assert_eq!(session.session_id, session_id);
        assert_eq!(session.initial_entry, start);
        assert_eq!(session.session_start.as_ref().unwrap(), &session.initial_entry);
        assert_eq!(session.final_entry, end);
        assert_eq!(session.session_end.as_ref().unwrap(), &session.final_entry);
        assert_eq!(session.app_id.as_ref().unwrap(), "PremierePro1");
        assert_eq!(session.app_version.as_ref().unwrap(), "22.5.0");
        assert_eq!(session.ngl_version.as_ref().unwrap(), "1.30.0.1");
        assert_eq!(session.app_locale.as_ref().unwrap(), "en_US");
        assert_eq!(
            session.user_id.as_ref().unwrap(),
            "9f22a90139cbb9f1676b0113e1fb574976dc550a"
        );
    }

    #[test]
    fn test_parse_partial_log_upload() {
        let path = "../rsrc/logs/mac/NGLClient_AcrobatDC122.1.20169.7.log.bin";
        let data = bytes::Bytes::from(read_to_string(path).unwrap());
        let sessions = super::parse_log_data(data);
        assert_eq!(sessions.len(), 1);
        let session = &sessions[0];
        let session_id = "e6ab2d44-5909-4838-a79f-5091f5736073.1659806990834";
        let start = Timestamp::from_storage("2022-08-08T09:25:33:720-0700");
        let end = Timestamp::from_storage("2022-08-08T09:25:33:720-0700");
        assert_eq!(session.session_id, session_id);
        assert_eq!(session.initial_entry, start);
        assert!(session.session_start.is_none());
        assert_eq!(session.final_entry, end);
        assert_eq!(session.session_end.as_ref().unwrap(), &session.final_entry);
        assert!(session.app_id.is_none());
        assert!(session.app_version.is_none());
        assert!(session.ngl_version.is_none());
        assert!(session.app_locale.is_none());
        assert!(session.user_id.is_none());
    }

    #[test]
    fn test_parse_win_unterminated_log_upload() {
        let path = "../rsrc/logs/win/NGLClient_Illustrator126.4.1.log.bin";
        let data = bytes::Bytes::from(read_to_string(path).unwrap());
        let sessions = super::parse_log_data(data);
        assert_eq!(sessions.len(), 1);
        let session = &sessions[0];
        let session_id = "bc532766-d56c-43fe-aaba-eb5f4323a53c.1660495166236";
        let start = Timestamp::from_storage("2022-08-14T09:39:26:236-0700");
        let end = Timestamp::from_storage("2022-08-14T09:39:45:536-0700");
        assert_eq!(session.session_id, session_id);
        assert_eq!(session.initial_entry, start);
        assert_eq!(session.session_start.as_ref().unwrap(), &session.initial_entry);
        assert_eq!(session.final_entry, end);
        assert!(session.session_end.is_none());
        assert_eq!(session.app_id.as_ref().unwrap(), "Illustrator1");
        assert_eq!(session.app_version.as_ref().unwrap(), "26.4.1");
        assert_eq!(session.ngl_version.as_ref().unwrap(), "1.30.0.2");
        assert_eq!(session.app_locale.as_ref().unwrap(), "en_US");
        assert_eq!(
            session.user_id.as_ref().unwrap(),
            "9f22a90139cbb9f1676b0113e1fb574976dc550a"
        );
    }
}
