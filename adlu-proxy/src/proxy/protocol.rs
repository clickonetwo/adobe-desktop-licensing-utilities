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
use warp::{Filter, Rejection};

use super::config::Config;

pub enum RequestType {
    FrlActivation,
    FrlDeactivation,
    NulLicense,
    LogUpload,
    Unknown,
}

pub struct Request {
    pub kind: RequestType,
    pub source_ip: Option<std::net::SocketAddr>,
    pub method: http::method::Method,
    pub path: String,
    pub query: Option<String>,
    pub body: Option<String>,
    pub content_type: String,
    pub accept_type: String,
    pub api_key: String,
    pub request_id: Option<String>,
    pub session_id: Option<String>,
    pub authorization: Option<String>,
}

impl Request {
    pub fn generic_filter() -> impl Filter<Extract = (Self,), Error = Rejection> + Clone {
        warp::any()
            .and(warp::filters::addr::remote())
            .and(warp::method())
            .and(warp::path::full())
            .and(optional_raw_query_filter())
            .and(warp::header::<String>("Content-Type"))
            .and(warp::header::<String>("Accept"))
            .and(warp::header::<String>("X-Api-Key"))
            .and(warp::filters::header::optional::<String>("X-Request-Id"))
            .and(warp::filters::header::optional::<String>("X-Session-Id"))
            .and(warp::filters::header::optional::<String>("Authorization"))
            .and(warp::body::content_length_limit(32_000))
            .and(warp::body::bytes())
            .map(
                |source_ip,
                 method,
                 path: warp::path::FullPath,
                 query,
                 content_type,
                 accept_type,
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
                        kind: RequestType::Unknown,
                        source_ip,
                        method,
                        path: path.as_str().to_string(),
                        query,
                        content_type,
                        accept_type,
                        api_key,
                        request_id,
                        session_id,
                        authorization,
                        body,
                    }
                },
            )
    }

    pub async fn send_to_adobe(&self, conf: &Config) -> Result<reqwest::Response> {
        let server = match self.kind {
            RequestType::LogUpload => conf.log_server.as_str(),
            _ => conf.frl_server.as_str(),
        };
        let endpoint = format!("{}/{}", server, &self.path);
        let mut builder = conf.client.request(self.method.clone(), &endpoint);
        if let Some(query) = &self.query {
            builder = builder.query(query);
        }
        builder = builder
            .header("User-Agent", super::agent())
            .header("Accept-Encoding", "gzip, deflate, br")
            .header("Accept", &self.accept_type)
            .header("Accept-Language", "en-us")
            .header("Content-Type", &self.content_type)
            .header("X-Api-Key", &self.api_key);
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

fn optional_raw_query_filter(
) -> impl Filter<Extract = (Option<String>,), Error = Rejection> + Clone {
    warp::query::raw()
        .map(Some)
        .or_else(|_| async { Ok::<(Option<String>,), Rejection>((None,)) })
}

#[cfg(test)]
mod test {
    use std::net;

    #[tokio::test]
    async fn generic_post_json_body() {
        let filter = super::Request::generic_filter();
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
            .expect("generic_request filter failed a JSON post");
        assert_eq!(req.query, None);
        assert_eq!(req.authorization, None);
        assert_eq!(req.api_key, "ngl_photoshop1");
    }
}
