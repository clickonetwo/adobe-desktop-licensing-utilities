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

use eyre::{Result, WrapErr};
use log::warn;
use warp::{Filter, Rejection, Reply};

use adlu_base::Timestamp;

use super::config::Config;

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
    pub content_type: String,
    pub accept_type: Option<String>,
    pub accept_language: Option<String>,
    pub user_agent: Option<String>,
    pub via: Option<String>,
    pub api_key: Option<String>,
    pub request_id: Option<String>,
    pub session_id: Option<String>,
    pub authorization: Option<String>,
}

impl Request {
    pub fn frl_activation_filter(
    ) -> impl Filter<Extract = (Self,), Error = Rejection> + Clone {
        warp::post()
            .and(warp::path!("asnp" / "frl_connected" / "values" / "v2"))
            .and(required_header("X-Api-Key"))
            .and(required_header("X-Request-Id"))
            .and(Self::request_filter(RequestType::FrlActivation))
    }

    pub fn frl_deactivation_filter(
    ) -> impl Filter<Extract = (Self,), Error = Rejection> + Clone {
        warp::delete()
            .and(warp::path!("asnp" / "frl_connected" / "v1"))
            .and(required_header("X-Api-Key"))
            .and(required_header("X-Request-Id"))
            .and(required_query())
            .and(Self::request_filter(RequestType::FrlDeactivation))
    }

    pub fn nul_license_filter(
    ) -> impl Filter<Extract = (Self,), Error = Rejection> + Clone {
        warp::post()
            .and(warp::path!("asnp" / "nud"))
            .and(required_header("X-Api-Key"))
            .and(required_header("X-Request-Id"))
            .and(required_header("X-Session-Id"))
            .and(required_header("Authorization"))
            .and(Self::request_filter(RequestType::FrlDeactivation))
    }

    pub fn log_upload_filter() -> impl Filter<Extract = (Self,), Error = Rejection> + Clone
    {
        warp::post()
            .and(warp::path!("ulecs" / "v1"))
            .and(required_header("X-Api-Key"))
            .and(required_header("Authorization"))
            .and(Self::request_filter(RequestType::FrlDeactivation))
    }

    pub fn unknown_filter() -> impl Filter<Extract = (Self,), Error = Rejection> + Clone {
        warp::any().and(Self::request_filter(RequestType::Unknown))
    }

    fn request_filter(
        kind: RequestType,
    ) -> impl Filter<Extract = (Self,), Error = Rejection> + Clone {
        warp::filters::addr::remote()
            .and(warp::method())
            .and(warp::path::full())
            .and(optional_raw_query())
            .and(warp::header::<String>("Content-Type"))
            .and(warp::filters::header::optional::<String>("Accept"))
            .and(warp::filters::header::optional::<String>("Accept-Language"))
            .and(warp::filters::header::optional::<String>("User-Agent"))
            .and(warp::filters::header::optional::<String>("Via"))
            .and(warp::filters::header::optional::<String>("X-Api-Key"))
            .and(warp::filters::header::optional::<String>("X-Request-Id"))
            .and(warp::filters::header::optional::<String>("X-Session-Id"))
            .and(warp::filters::header::optional::<String>("Authorization"))
            .and(warp::body::content_length_limit(32_000))
            .and(warp::body::bytes())
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
                      body: bytes::Bytes| {
                    let body: Option<String> = if body.is_empty() {
                        None
                    } else {
                        Some(String::from_utf8_lossy(&body).to_string())
                    };
                    Self {
                        timestamp: Timestamp::now(),
                        request_type: kind.clone(),
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

    pub async fn send_to_adobe(&self, conf: &Config) -> Result<reqwest::Response> {
        let server = match self.request_type {
            RequestType::LogUpload => conf.log_server.as_str(),
            _ => conf.frl_server.as_str(),
        };
        let endpoint = format!("{}/{}", server, &self.path);
        let mut builder = conf.client.request(self.method.clone(), &endpoint);
        if let Some(query) = &self.query {
            builder = builder.query(query);
        }
        builder = builder
            .header("Accept-Encoding", "gzip, deflate, br")
            .header("Content-Type", &self.content_type);
        if let Some(accept_type) = &self.accept_type {
            builder = builder.header("Accept", accept_type)
        }
        if let Some(accept_language) = &self.accept_language {
            builder = builder.header("Accept-Language", accept_language);
        }
        if let Some(api_key) = &self.api_key {
            builder = builder.header("X-Api-Key", api_key);
        }
        if let Some(request_id) = &self.request_id {
            builder = builder.header("X-Request-Id", request_id);
        }
        if let Some(session_id) = &self.session_id {
            builder = builder.header("X-Session-Id", session_id);
        }
        if let Some(authorization) = &self.authorization {
            builder = builder.header("Authorization", authorization);
        }
        if let Some(body) = &self.body {
            builder = builder.body(body.clone())
        }
        let request = builder.build().wrap_err("Error creating network request")?;
        if cfg!(test) {
            super::mock_adobe_server(conf, request)
                .await
                .wrap_err("Error mocking network request")
        } else {
            conf.client.execute(request).await.wrap_err("Error executing network request")
        }
    }
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

#[derive(Clone, Debug)]
pub struct Response {
    pub timestamp: Timestamp,
    pub request_type: RequestType,
    pub status: http::status::StatusCode,
    pub body: Option<String>,
    pub content_type: Option<String>,
    pub server: Option<String>,
    pub via: Option<String>,
    pub request_id: Option<String>,
    pub session_id: Option<String>,
}

impl From<Response> for warp::reply::Response {
    fn from(resp: Response) -> Self {
        let mut builder = http::Response::builder().status(resp.status.clone());
        if let Some(server) = resp.server {
            builder = builder.header("Server", server);
        }
        if let Some(via) = resp.via {
            builder = builder.header("Via", format!("{}, {}", via, super::proxy_id()));
        } else {
            builder = builder.header("Via", super::proxy_id());
        }
        if let Some(request_id) = resp.request_id {
            builder = builder.header("X-Request-Id", request_id);
        }
        if let Some(session_id) = resp.session_id {
            builder = builder.header("X-Session-Id", session_id);
        }
        if let Some(content) = resp.body {
            if let Some(content_type) = resp.content_type {
                builder = builder.header("Content-Type", content_type);
            }
            builder.body(content.into()).unwrap()
        } else {
            builder.body("".into()).unwrap()
        }
    }
}

impl Reply for Response {
    fn into_response(self) -> warp::reply::Response {
        self.into()
    }
}

impl Response {
    pub async fn from_network(req: &Request, resp: reqwest::Response) -> Result<Self> {
        let timestamp = Timestamp::now();
        let request_type = req.request_type.clone();
        let status = resp.status();
        let content_type = if let Some(val) = resp.headers().get("Content-Type") {
            Some(val.to_str().wrap_err("Content-Type is not valid")?.to_string())
        } else {
            None
        };
        let server = if let Some(val) = resp.headers().get("Server") {
            Some(val.to_str().wrap_err("Server is not valid")?.to_string())
        } else {
            None
        };
        let via = if let Some(val) = resp.headers().get("Via") {
            Some(val.to_str().wrap_err("Via is not valid")?.to_string())
        } else {
            None
        };
        let request_id = if let Some(val) = resp.headers().get("X-Request-Id") {
            Some(val.to_str().wrap_err("X-Request-Id is not valid")?.to_string())
        } else {
            None
        };
        let session_id = if let Some(val) = resp.headers().get("X-Session-Id") {
            Some(val.to_str().wrap_err("X-Request-Id is not valid")?.to_string())
        } else {
            None
        };
        let content = resp.text().await.wrap_err("Failure to receive body")?;
        let body = if content.is_empty() {
            None
        } else {
            if !content_type.is_some() {
                warn!("Response has body but no content type");
            }
            Some(content)
        };
        Ok(Self {
            timestamp,
            request_type,
            status,
            body,
            content_type,
            server,
            via,
            request_id,
            session_id,
        })
    }
}

#[cfg(test)]
mod test {
    use std::net;

    #[tokio::test]
    async fn protocol_generic_post_json_body() {
        let filter = super::Request::unknown_filter();
        let req = warp::test::request()
            .remote_addr("127.0.0.1:18040".parse::<net::SocketAddr>().unwrap())
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
    async fn protocol_missing_content_type_reject() {
        let filter = super::Request::unknown_filter();
        let req = warp::test::request()
            .remote_addr("127.0.0.1:18040".parse::<net::SocketAddr>().unwrap())
            .method("POST")
            .path("/asnp/v1")
            .header("User-Agent", "TestAgent")
            .header("Accept", "application/json")
            .header("Accept-Encoding", "deflate")
            .body(r#"{"key1": "value1", "key2": 300}"#)
            .filter(&filter)
            .await;
        assert!(req.is_err());
    }
}
