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
mod frl;
mod log;
mod named_user;

use eyre::{eyre, Result, WrapErr};
use warp::{Filter, Reply};

use adlu_base::Timestamp;
pub use frl::{
    FrlActivationRequest, FrlActivationRequestBody, FrlActivationResponse,
    FrlActivationResponseBody, FrlAppDetails, FrlDeactivationQueryParams,
    FrlDeactivationRequest, FrlDeactivationResponse, FrlDeactivationResponseBody,
    FrlDeviceDetails,
};
pub use log::{parse_log_data, LogSession, LogUploadRequest, LogUploadResponse};
pub use named_user::{
    NulActivationRequest, NulActivationRequestBody, NulActivationResponse,
    NulActivationResponseBody, NulAppDetails, NulDeactivationRequest,
    NulDeactivationResponse, NulDeactivationResponseBody, NulDeviceDetails,
};

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
                let request_id = format!("{}.{}", api_key, timestamp.to_millis());
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

fn get_response_id(response: &reqwest::Response) -> Result<String> {
    match response.headers().get("X-Request-Id") {
        None => Err(eyre!("No request-id")),
        Some(val) => Ok(val.to_str().wrap_err("Invalid request-id")?.to_string()),
    }
}
