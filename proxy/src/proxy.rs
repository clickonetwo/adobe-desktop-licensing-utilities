/*
Copyright 2020 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
pub mod plain;
pub mod secure;

// use futures::TryStreamExt;
use crate::cache::Cache;
use crate::cops::{agent, BadRequest, Request as CRequest, Response as CResponse};
use hyper::{Body, Client, Request as HRequest, Response as HResponse, Uri};
use hyper_tls::HttpsConnector;
use log::{debug, error, info};
use std::sync::{Arc, Mutex};

use crate::settings::Settings;

fn ctrl_c_handler<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    let call_once = Mutex::new(Some(f));

    ctrlc::set_handler(move || {
        if let Some(f) = call_once.lock().unwrap().take() {
            info!("Starting graceful shutdown");
            f();
        } else {
            info!("Already sent signal to start graceful shutdown");
        }
    })
    .unwrap();
}

async fn serve_req(
    req: HRequest<Body>, conf: Settings, cache: Arc<Cache>,
) -> Result<HResponse<Body>, hyper::Error> {
    let (parts, body) = req.into_parts();
    let body = hyper::body::to_bytes(body).await?;
    info!("Received request for {:?}", parts.uri);
    debug!("Received request method: {:?}", parts.method);
    debug!("Received request headers: {:?}", parts.headers);
    debug!(
        "Received request body: {}",
        std::str::from_utf8(&body).unwrap()
    );

    // Analyze and handle the request
    match CRequest::from_network(&parts, &body) {
        Err(err) => Ok(bad_request_response(&err)),
        Ok(req) => {
            info!("Received request id: {}", &req.request_id);
            cache.store_request(&req).await;
            let net_resp = if conf.proxy.mode.starts_with('s') {
                debug!("Store mode - not contacting COPS");
                proxy_offline_response()
            } else {
                match call_cops(&conf, &req).await {
                    Ok(resp) => resp,
                    Err(err) => cops_failure_response(err),
                }
            };
            let (parts, body) = net_resp.into_parts();
            let body = hyper::body::to_bytes(body).await?;
            if parts.status.is_success() {
                // the COPS call succeeded,
                info!("Received success response ({:?}) from COPS", parts.status);
                debug!("Received success response headers {:?}", parts.headers);
                debug!(
                    "Received success response body {}",
                    std::str::from_utf8(&body).unwrap()
                );
                // cache the response
                let resp = CResponse::from_network(&req, &body);
                cache.store_response(&req, &resp).await;
                // return the response
                Ok(HResponse::from_parts(parts, Body::from(body)))
            } else if let Some(resp) = cache.fetch_response(&req).await {
                // COPS call failed, but we have a cached response to use
                info!("Using previously cached response to request");
                let net_resp = resp.to_network();
                Ok(net_resp)
            } else {
                // COPS call failed, and no cache, so tell client
                info!("Returning failure response ({:?}) from COPS", parts.status);
                debug!("Received failure response headers {:?}", parts.headers);
                debug!(
                    "Received failure response body {}",
                    std::str::from_utf8(&body).unwrap()
                );
                Ok(HResponse::from_parts(parts, Body::from(body)))
            }
        }
    }
}

pub async fn forward_stored_requests(conf: &Settings, cache: Arc<Cache>) {
    let requests = cache.fetch_stored_requests().await;
    for req in requests.iter() {
        info!("Forwarding stored {} request {}", req.kind, &req.request_id);
        match call_cops(&conf, &req).await {
            Ok(net_resp) => {
                let (parts, body) = net_resp.into_parts();
                let body = hyper::body::to_bytes(body).await.unwrap();
                if parts.status.is_success() {
                    // the COPS call succeeded,
                    info!("Received success response ({:?}) from COPS", parts.status);
                    debug!("Received success response headers {:?}", parts.headers);
                    debug!(
                        "Received success response body {}",
                        std::str::from_utf8(&body).unwrap()
                    );
                    // cache the response
                    let resp = CResponse::from_network(&req, &body);
                    cache.store_response(&req, &resp).await;
                } else {
                    // the COPS call failed
                    info!("Received failure response ({:?}) from COPS", parts.status);
                    debug!("Received failure response headers {:?}", parts.headers);
                    debug!(
                        "Received failure response body {}",
                        std::str::from_utf8(&body).unwrap()
                    );
                }
            }
            Err(err) => {
                error!("No response received from COPS: {:?}", err)
            }
        };
    }
}

async fn call_cops(
    conf: &Settings, req: &CRequest,
) -> Result<HResponse<Body>, hyper::Error> {
    let cops_uri =
        conf.proxy.remote_host.parse::<Uri>().unwrap_or_else(|_| {
            panic!("failed to parse uri: {}", conf.proxy.remote_host)
        });

    // if no scheme is specified for remote_host, assume http
    let cops_scheme = match cops_uri.scheme_str() {
        Some("https") => "https",
        _ => "http",
    };

    let cops_host = match cops_uri.port() {
        Some(port) => {
            let h = cops_uri.host().unwrap();
            format!("{}:{}", h, port.as_str())
        }
        None => String::from(cops_uri.host().unwrap()),
    };

    info!(
        "Forwarding request {} to COPS at {}://{}",
        req.request_id, cops_scheme, cops_host
    );
    let net_req = req.to_network(&cops_scheme, &cops_host);
    if cops_scheme == "https" {
        let https = HttpsConnector::new();
        let client = Client::builder().build::<_, hyper::Body>(https);
        client.request(net_req).await
    } else {
        Client::new().request(net_req).await
    }
}

fn bad_request_response(err: &BadRequest) -> HResponse<Body> {
    info!("Rejecting request with 400 response: {}", err.reason);
    let body = serde_json::json!({"statusCode": 400, "message": err.reason});
    HResponse::builder()
        .status(400)
        .header("content-type", "application/json;charset=UTF-8")
        .header("server", agent())
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn cops_failure_response(err: hyper::Error) -> HResponse<Body> {
    let msg = format!("Failed to get a response from COPS: {:?}", err);
    error!("{}", msg);
    let body = serde_json::json!({"statusCode": 502, "message": msg});
    HResponse::builder()
        .status(502)
        .header("content-type", "application/json;charset=UTF-8")
        .header("server", agent())
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn proxy_offline_response() -> HResponse<Body> {
    let msg = "Proxy is operating offline: request stored for later replay";
    debug!("{}", msg);
    let body = serde_json::json!({"statusCode": 502, "message": msg});
    HResponse::builder()
        .status(502)
        .header("content-type", "application/json;charset=UTF-8")
        .header("server", agent())
        .body(Body::from(body.to_string()))
        .unwrap()
}
