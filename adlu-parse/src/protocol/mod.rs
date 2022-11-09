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
use warp::{filters::BoxedFilter, Filter, Rejection};

use adlu_base::Timestamp;
pub use frl::{
    FrlActivationRequestBody, FrlActivationResponseBody, FrlAppDetails,
    FrlDeactivationQueryParams, FrlDeactivationResponseBody, FrlDeviceDetails,
};
pub use log::{LogSession, LogUploadResponse};
pub use named_user::{
    LicenseSession, NulAppDetails, NulDeviceDetails, NulLicenseRequestBody,
    NulLicenseResponseBody,
};

mod frl;
mod log;
mod named_user;

#[derive(Clone, Debug)]
pub enum RequestType {
    FrlActivation,
    FrlDeactivation,
    NulLicense,
    LogUpload,
    Unknown,
}

impl std::fmt::Display for RequestType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestType::FrlActivation => write!(f, "FRL Activation"),
            RequestType::FrlDeactivation => write!(f, "FRL Deactivation"),
            RequestType::NulLicense => write!(f, "NUL License"),
            RequestType::LogUpload => write!(f, "Log Upload"),
            RequestType::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Request {
    pub timestamp: Timestamp,
    pub request_type: RequestType,
    pub source_ip: Option<std::net::SocketAddr>,
    pub method: http::method::Method,
    pub path: String,
    pub query: Option<String>,
    pub body: Option<String>,
    pub content_type: Option<String>,
    pub accept_type: Option<String>,
    pub accept_language: Option<String>,
    pub user_agent: Option<String>,
    pub via: Option<String>,
    pub api_key: Option<String>,
    pub request_id: Option<String>,
    pub session_id: Option<String>,
    pub authorization: Option<String>,
}

impl std::fmt::Display for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} request {}", self.request_type, self.with_id())
    }
}

impl Request {
    pub fn frl_activation_boxed_filter() -> BoxedFilter<(Self,)> {
        Request::frl_activation_filter().boxed()
    }

    pub fn frl_activation_filter(
    ) -> impl Filter<Extract = (Self,), Error = Rejection> + Clone {
        warp::post()
            .and(warp::path!("asnp" / "frl_connected" / "values" / "v2"))
            .and(required_header("X-Api-Key"))
            .and(required_header("X-Request-Id"))
            .and(Self::request_boxed_filter(RequestType::FrlActivation))
    }

    pub fn frl_deactivation_boxed_filter() -> BoxedFilter<(Self,)> {
        Request::frl_deactivation_filter().boxed()
    }

    pub fn frl_deactivation_filter(
    ) -> impl Filter<Extract = (Self,), Error = Rejection> + Clone {
        warp::delete()
            .and(warp::path!("asnp" / "frl_connected" / "v1"))
            .and(required_header("X-Api-Key"))
            .and(required_header("X-Request-Id"))
            .and(required_query())
            .and(Self::request_boxed_filter(RequestType::FrlDeactivation))
    }

    pub fn nul_license_boxed_filter() -> BoxedFilter<(Self,)> {
        Request::nul_license_filter().boxed()
    }

    pub fn nul_license_filter(
    ) -> impl Filter<Extract = (Self,), Error = Rejection> + Clone {
        warp::post()
            .and(warp::path("asnp"))
            .and(warp::path("nud"))
            .and(required_header("X-Api-Key"))
            .and(required_header("X-Request-Id"))
            .and(required_header("X-Session-Id"))
            .and(required_header("Authorization"))
            .and(Self::request_boxed_filter(RequestType::NulLicense))
    }

    pub fn log_upload_boxed_filter() -> BoxedFilter<(Self,)> {
        Self::log_upload_filter().boxed()
    }

    pub fn log_upload_filter() -> impl Filter<Extract = (Self,), Error = Rejection> + Clone
    {
        warp::post()
            .and(warp::path!("ulecs" / "v1"))
            .and(required_header("X-Api-Key"))
            .and(required_header("Authorization"))
            .and(Self::request_boxed_filter(RequestType::LogUpload))
    }

    pub fn unknown_boxed_filter() -> BoxedFilter<(Self,)> {
        Self::unknown_filter().boxed()
    }

    pub fn unknown_filter() -> impl Filter<Extract = (Self,), Error = Rejection> + Clone {
        warp::any().and(Self::request_boxed_filter(RequestType::Unknown))
    }

    fn request_boxed_filter(request_type: RequestType) -> BoxedFilter<(Self,)> {
        Self::request_filter(request_type).boxed()
    }

    fn request_filter(
        request_type: RequestType,
    ) -> impl Filter<Extract = (Self,), Error = Rejection> + Clone {
        proxied_remote_addr()
            .and(warp::method())
            .and(warp::path::full())
            .and(optional_raw_query())
            .and(warp::filters::header::optional::<String>("Content-Type"))
            .and(warp::filters::header::optional::<String>("Accept"))
            .and(warp::filters::header::optional::<String>("Accept-Language"))
            .and(warp::filters::header::optional::<String>("User-Agent"))
            .and(warp::filters::header::optional::<String>("Via"))
            .and(warp::filters::header::optional::<String>("X-Api-Key"))
            .and(warp::filters::header::optional::<String>("X-Request-Id"))
            .and(warp::filters::header::optional::<String>("X-Session-Id"))
            .and(warp::filters::header::optional::<String>("Authorization"))
            .and(optional_body_filter())
            .map(
                move |source_ip,
                      method,
                      path: warp::path::FullPath,
                      query,
                      content_type,
                      accept_type,
                      accept_language,
                      user_agent,
                      via,
                      api_key,
                      request_id,
                      session_id,
                      authorization,
                      body| {
                    Self {
                        timestamp: Timestamp::now(),
                        request_type: request_type.clone(),
                        source_ip,
                        method,
                        path: path.as_str().to_string(),
                        query,
                        content_type,
                        accept_type,
                        accept_language,
                        user_agent,
                        via,
                        api_key,
                        request_id,
                        session_id,
                        authorization,
                        body,
                    }
                },
            )
    }

    pub fn with_id(&self) -> String {
        if let Some(request_id) = &self.request_id {
            format!("with X-Request-Id: {}", request_id)
        } else {
            format!("with Timestamp: {:x}", self.timestamp.millis)
        }
    }
}

fn optional_body_filter(
) -> impl Filter<Extract = (Option<String>,), Error = std::convert::Infallible> + Clone {
    warp::body::content_length_limit(32_000)
        .and(warp::body::bytes())
        .map(|b: bytes::Bytes| Some(String::from_utf8_lossy(&b).to_string()))
        .or_else(|_| async { Ok::<(Option<String>,), std::convert::Infallible>((None,)) })
}

fn proxied_remote_addr(
) -> impl Filter<Extract = (Option<std::net::SocketAddr>,), Error = std::convert::Infallible>
       + Clone {
    warp::filters::header::optional::<std::net::SocketAddr>("X-Forwarded-For")
        .or(warp::filters::header::optional::<std::net::SocketAddr>("X-Real-Ip"))
        .unify()
        .or_else(|_| async {
            Ok::<(Option<std::net::SocketAddr>,), std::convert::Infallible>((None,))
        })
        .and(warp::filters::addr::remote())
        .map(|p: Option<std::net::SocketAddr>, r: Option<std::net::SocketAddr>| p.or(r))
}

fn optional_raw_query(
) -> impl Filter<Extract = (Option<String>,), Error = Rejection> + Clone {
    warp::query::raw()
        .map(Some)
        .or_else(|_| async { Ok::<(Option<String>,), Rejection>((None,)) })
}

fn required_query() -> impl Filter<Extract = (), Error = Rejection> + Clone {
    warp::query::raw().map(|_| {}).untuple_one()
}

fn required_header(
    key: &'static str,
) -> impl Filter<Extract = (), Error = Rejection> + Clone {
    warp::header::<String>(key).map(|_| {}).untuple_one()
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn protocol_generic_post_json_body() {
        let filter = super::Request::unknown_filter();
        let req = warp::test::request()
            .remote_addr("127.0.0.1:18040".parse::<std::net::SocketAddr>().unwrap())
            .method("POST")
            .path("/asnp/v1")
            .header("User-Agent", "TestAgent")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header("Accept-Encoding", "deflate")
            .header("X-Api-Key", "ngl_photoshop1")
            .header("X-Request-Id", "request1")
            .header("X-Session-Id", "session1")
            .body(r#"{"key1": "value1", "key2": 300}"#)
            .filter(&filter)
            .await
            .expect("unknown_filter failed a JSON post");
        assert_eq!(req.query, None);
        assert_eq!(req.authorization, None);
        assert_eq!(req.api_key.expect("No API Key"), "ngl_photoshop1");
    }

    #[tokio::test]
    async fn protocol_missing_content_type_accept() {
        let filter = super::Request::unknown_filter();
        warp::test::request()
            .remote_addr("127.0.0.1:18040".parse::<std::net::SocketAddr>().unwrap())
            .method("POST")
            .path("/asnp/v1")
            .header("User-Agent", "TestAgent")
            .header("Accept", "application/json")
            .header("Accept-Encoding", "deflate")
            .body(r#"{"key1": "value1", "key2": 300}"#)
            .filter(&filter)
            .await
            .expect("Req with no content-type was rejected");
    }

    #[tokio::test]
    async fn protocol_missing_content_length_accept() {
        let filter = super::Request::unknown_filter();
        let req = warp::test::request()
            .remote_addr("127.0.0.1:18040".parse::<std::net::SocketAddr>().unwrap())
            .method("GET")
            .path("/")
            .header("User-Agent", "TestAgent")
            .header("Accept", "application/json")
            .header("Accept-Encoding", "deflate")
            .filter(&filter)
            .await
            .expect("Request with no body was rejected");
        assert!(req.body.is_none());
    }
}
