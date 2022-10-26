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
    LicenseSession, NulActivationRequest, NulActivationRequestBody,
    NulActivationResponse, NulActivationResponseBody, NulAppDetails, NulDeviceDetails,
};

/// An enumeration type of protocol requests.
#[derive(Debug, Clone)]
pub enum Request {
    FrlActivation(Box<FrlActivationRequest>),
    FrlDeactivation(Box<FrlDeactivationRequest>),
    NulActivation(Box<NulActivationRequest>),
    LogUpload(Box<LogUploadRequest>),
}

impl Request {
    pub fn timestamp(&self) -> &Timestamp {
        match self {
            Request::FrlActivation(req) => &req.timestamp,
            Request::FrlDeactivation(req) => &req.timestamp,
            Request::NulActivation(req) => &req.timestamp,
            Request::LogUpload(req) => &req.timestamp,
        }
    }

    pub fn request_id(&self) -> &str {
        match self {
            Request::FrlActivation(req) => &req.request_id,
            Request::FrlDeactivation(req) => &req.request_id,
            Request::NulActivation(req) => &req.request_id,
            Request::LogUpload(req) => &req.request_id,
        }
    }

    /// a [`warp::Filter`] that produces an `FrlActivationRequest` from a well-formed FRL
    /// activation network request posted to a warp server.  You can compose this filter with
    /// a path filter to provide an activation handler at a specific endpoint.
    pub fn frl_activation_filter(
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
                    Request::FrlActivation(Box::new(FrlActivationRequest {
                        timestamp: Timestamp::now(),
                        api_key,
                        request_id,
                        session_id,
                        parsed_body,
                    }))
                },
            )
    }

    /// a [`warp::Filter`] that produces an `FrlDeactivationRequest` from a well-formed FRL
    /// deactivation network request posted to a warp server.  You can compose this filter with
    /// a path filter to provide a deactivation handler at a specific endpoint.
    pub fn frl_deactivation_filter(
    ) -> impl Filter<Extract = (Self,), Error = warp::Rejection> + Clone {
        warp::delete()
            .and(warp::header::<String>("X-Request-Id"))
            .and(warp::header::<String>("X-Api-Key"))
            .and(warp::query::<FrlDeactivationQueryParams>())
            .map(
                |request_id: String,
                 api_key: String,
                 params: FrlDeactivationQueryParams| {
                    Request::FrlDeactivation(Box::new(FrlDeactivationRequest {
                        timestamp: Timestamp::now(),
                        api_key,
                        request_id,
                        params,
                    }))
                },
            )
    }

    /// a [`warp::Filter`] that produces an `NulActivationRequest` from a well-formed FRL
    /// activation network request posted to a warp server.  You can compose this filter with
    /// a path filter to provide an activation handler at a specific endpoint.
    pub fn nul_activation_filter(
    ) -> impl Filter<Extract = (Self,), Error = warp::Rejection> + Clone {
        warp::post()
            .and(warp::header::<String>("Authorization"))
            .and(warp::header::<String>("X-Session-Id"))
            .and(warp::header::<String>("X-Request-Id"))
            .and(warp::header::<String>("X-Api-Key"))
            .and(warp::body::content_length_limit(20 * 1024))
            .and(warp::body::bytes())
            .map(
                |authorization: String,
                 session_id: String,
                 request_id: String,
                 api_key: String,
                 body: bytes::Bytes| {
                    Request::NulActivation(Box::new(NulActivationRequest::from_parts(
                        authorization,
                        request_id,
                        session_id,
                        api_key,
                        body,
                    )))
                },
            )
    }

    /// a [`warp::Filter`] that produces a `LogUploadRequest` from a well-formed
    /// log upload network request posted to a warp server.  You can compose this filter with
    /// a log upload filter to provide a log upload handler at a specific endpoint.
    pub fn log_upload_filter(
    ) -> impl Filter<Extract = (Self,), Error = warp::Rejection> + Clone {
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
            Request::FrlActivation(req) => req.to_network(builder),
            Request::FrlDeactivation(req) => req.to_network(builder),
            Request::NulActivation(req) => req.to_network(builder),
            Request::LogUpload(req) => req.to_network(builder),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Response {
    FrlActivation(Box<FrlActivationResponse>),
    FrlDeactivation(Box<FrlDeactivationResponse>),
    NulActivation(Box<NulActivationResponse>),
    LogUpload(Box<LogUploadResponse>),
}

impl Response {
    pub fn timestamp(&self) -> &Timestamp {
        match self {
            Response::FrlActivation(resp) => &resp.timestamp,
            Response::FrlDeactivation(resp) => &resp.timestamp,
            Response::NulActivation(resp) => &resp.timestamp,
            Response::LogUpload(resp) => &resp.timestamp,
        }
    }

    pub async fn from_network(req: &Request, resp: reqwest::Response) -> Result<Self> {
        match req {
            Request::FrlActivation(_) => {
                let resp = FrlActivationResponse::from_network(resp).await?;
                Ok(Response::FrlActivation(Box::new(resp)))
            }
            Request::FrlDeactivation(_) => {
                let resp = FrlDeactivationResponse::from_network(resp).await?;
                Ok(Response::FrlDeactivation(Box::new(resp)))
            }
            Request::NulActivation(_) => {
                let resp = NulActivationResponse::from_network(resp).await?;
                Ok(Response::NulActivation(Box::new(resp)))
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
            Response::FrlActivation(resp) => resp.into_response(),
            Response::FrlDeactivation(resp) => resp.into_response(),
            Response::NulActivation(resp) => resp.into_response(),
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
