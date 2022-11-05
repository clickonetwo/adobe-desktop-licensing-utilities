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

/*!
Provides the top-level proxy framework, both insecure and secure.  This includes a status endpoint
that can be used to ensure the proxy is up and find out which services it is providing.
 */
use std::convert::Infallible;

use eyre::{eyre, Report, Result};
use log::{debug, error, info};
use serde_json::{json, Value};
use warp::{reply, Filter, Rejection, Reply};

use crate::cache::Cache;
use crate::settings::{ProxyMode, Settings};

pub use self::config::Config;
pub use self::protocol::{Request, RequestType, Response};

pub mod config;
pub mod protocol;

pub async fn serve_incoming_https_requests(
    settings: &Settings,
    cache: &Cache,
    stop_signal: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<()> {
    let conf = Config::new(settings.clone(), cache.clone())?;
    let routes = routes(conf.clone());
    let bind_addr = conf.bind_addr()?;
    openssl_probe::init_ssl_cert_env_vars();
    let cert_data = conf.cert_data()?;
    let server =
        warp::serve(routes).tls().cert(cert_data.cert_pem()).key(cert_data.key_pem());
    let (addr, server) = server.bind_with_graceful_shutdown(bind_addr, stop_signal);
    info!(
        "adlu-proxy v{} serving HTTPS requests on {:?}...",
        env!("CARGO_PKG_VERSION"),
        addr
    );
    match tokio::task::spawn(server).await {
        Ok(_) => info!("HTTPS server terminated normally"),
        Err(err) => error!("HTTPS server terminated abnormally: {:?}", err),
    }
    Ok(())
}

pub async fn serve_incoming_http_requests(
    settings: &Settings,
    cache: &Cache,
    stop_signal: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<()> {
    let conf = Config::new(settings.clone(), cache.clone())?;
    let routes = routes(conf.clone());
    let bind_addr = conf.bind_addr()?;
    let (addr, server) =
        warp::serve(routes).bind_with_graceful_shutdown(bind_addr, stop_signal);
    info!(
        "adlu-proxy v{} serving HTTP requests on {:?}...",
        env!("CARGO_PKG_VERSION"),
        addr
    );
    match tokio::task::spawn(server).await {
        Ok(_) => info!("HTTP server terminated normally"),
        Err(err) => error!("HTTP server terminated abnormally: {:?}", err),
    }
    Ok(())
}

pub async fn forward_stored_requests(settings: &Settings, cache: &Cache) -> Result<()> {
    let conf = Config::new(settings.clone(), cache.clone())?;
    let reqs = conf.cache.fetch_unanswered_requests().await?;
    if reqs.is_empty() {
        info!("No requests to forward.");
        eprintln!("No requests to forward.");
        return Ok(());
    }
    let count = reqs.len();
    eprintln!("Found {} request(s) to forward", count);
    let (mut successes, mut failures) = (0u64, 0u64);
    for req in reqs.iter() {
        if forward_stored_request(&conf, req).await {
            successes += 1
        } else {
            failures += 1
        }
    }
    eprintln!(
        "Forwarding produced {} success(es) and {} failure(s).",
        successes, failures
    );
    Ok(())
}

pub fn routes(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    status_route(conf.clone()).or(adobe_route(conf)).with(warp::log("route::summary"))
}

pub fn with_conf(
    conf: Config,
) -> impl Filter<Extract = (Config,), Error = Infallible> + Clone {
    warp::any().map(move || conf.clone())
}

pub fn status_route(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::get()
        .and(warp::path("status"))
        .and(warp::path::end())
        .and(with_conf(conf))
        .then(status)
}

pub fn adobe_route(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    Request::frl_activation_filter()
        .or(Request::frl_deactivation_filter())
        .unify()
        .or(Request::nul_license_filter())
        .unify()
        .or(Request::log_upload_filter())
        .unify()
        .or(Request::unknown_filter())
        .unify()
        .and(with_conf(conf))
        .then(process_adobe_request)
}

pub fn frl_activate_route(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    Request::frl_activation_filter().and(with_conf(conf)).then(process_adobe_request)
}

pub fn frl_deactivate_route(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    Request::frl_deactivation_filter().and(with_conf(conf)).then(process_adobe_request)
}

pub fn nul_license_route(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    Request::nul_license_filter().and(with_conf(conf)).then(process_adobe_request)
}

pub fn upload_route(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    Request::log_upload_filter().and(with_conf(conf)).then(process_adobe_request)
}

pub fn unknown_route(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    Request::unknown_filter().and(with_conf(conf)).then(process_adobe_request)
}

pub async fn status(conf: Config) -> reply::Response {
    let status = format!("{} running in {:?} mode", proxy_id(), conf.settings.proxy.mode);
    info!("Status request received, issuing status: {}", &status);
    let body = json!({"statusCode": 200, "status": &status});
    proxy_reply(http::StatusCode::OK, &body)
}

pub async fn process_adobe_request(req: Request, conf: Config) -> reply::Response {
    info!("Received {} request {}", &req.request_type, req.with_id());
    debug!("Received {} request: {:?}", &req.request_type, &req);
    if !matches!(conf.settings.proxy.mode, ProxyMode::Isolated) {
        conf.cache.store_request(&req).await;
    }
    match send_request(&conf, &req).await {
        SendOutcome::Success(resp) => resp.into_response(),
        SendOutcome::Isolated => proxy_offline_reply(),
        SendOutcome::Unreachable(err) => unreachable_reply(err),
        SendOutcome::ParseFailure(err) => adobe_error_reply(err),
        SendOutcome::ErrorStatus(response) => adobe_bad_status_reply(response).await,
    }
}

pub async fn forward_stored_request(conf: &Config, req: &Request) -> bool {
    matches!(send_request(conf, req).await, SendOutcome::Success(_))
}

pub enum SendOutcome {
    Success(Response),
    Isolated,
    Unreachable(Report),
    ParseFailure(Report),
    ErrorStatus(reqwest::Response),
}

pub async fn send_request(conf: &Config, req: &Request) -> SendOutcome {
    let id = format!("{} request {}", &req.request_type, req.with_id());
    let outcome = if let ProxyMode::Isolated = conf.settings.proxy.mode {
        info!("Isolated - not forwarding {}", id);
        SendOutcome::Isolated
    } else {
        info!("Sending {} to Adobe endpoint", id);
        match req.send_to_adobe(conf).await {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    info!("Received valid response status for {}: {}", id, status);
                    match Response::from_network(req, response).await {
                        Ok(resp) => {
                            debug!("Response for {}: {:?}", id, resp);
                            // cache the response
                            conf.cache.store_response(&resp).await;
                            SendOutcome::Success(resp)
                        }
                        Err(err) => {
                            error!("Can't parse response for {}: {}", id, err);
                            SendOutcome::ParseFailure(err)
                        }
                    }
                } else {
                    error!("Received failure status for {}: {}", id, status);
                    debug!("Response for {}: {:?}", id, response);
                    // return the safe bits of the response
                    SendOutcome::ErrorStatus(response)
                }
            }
            Err(err) => {
                info!("Network failure sending {}", id);
                SendOutcome::Unreachable(err)
            }
        }
    };
    if let SendOutcome::Success(resp) = outcome {
        SendOutcome::Success(resp)
    } else if let Some(resp) = conf.cache.fetch_response(req).await {
        info!("Using previously cached response for {}", id);
        SendOutcome::Success(resp)
    } else {
        outcome
    }
}

#[cfg(test)]
async fn mock_adobe_server(
    conf: &Config,
    request: reqwest::Request,
) -> Result<reqwest::Response> {
    crate::testing::mock_adobe_server(conf, request).await
}

#[cfg(not(test))]
async fn mock_adobe_server(_: &Config, _: reqwest::Request) -> Result<reqwest::Response> {
    Err(eyre!("Can't mock except in testing"))
}

fn proxy_reply(status: http::StatusCode, body: &Value) -> reply::Response {
    reply::with_status(reply::with_header(reply::json(body), "Via", proxy_via()), status)
        .into_response()
}

fn proxy_offline_reply() -> reply::Response {
    let message = "Proxy is operating offline: request stored for later replay";
    debug!("{}", message);
    let body = json!({"statusCode": 502, "message": message});
    proxy_reply(http::StatusCode::BAD_GATEWAY, &body)
}

fn unreachable_reply(err: Report) -> reply::Response {
    let message = format!("Could not reach Adobe: {}", err);
    error!("{}", &message);
    let body = json!({"statusCode": 502, "message": message});
    proxy_reply(http::StatusCode::BAD_GATEWAY, &body)
}

async fn adobe_bad_status_reply(resp: reqwest::Response) -> reply::Response {
    let mut builder = http::Response::builder().status(resp.status());
    if let Some(request_id) = resp.headers().get("X-Request-Id") {
        builder = builder.header("X-Request-Id", request_id)
    }
    if let Some(content_type) = resp.headers().get("Content-Type") {
        builder = builder.header("Content-Type", content_type)
    }
    if let Some(via) = resp.headers().get("Via") {
        builder = builder.header(
            "Via",
            format!("{}, {}", via.to_str().unwrap_or("upstream"), proxy_via()),
        )
    } else {
        builder = builder.header("Via", proxy_via())
    }
    let body = match resp.bytes().await {
        Ok(val) => val,
        Err(err) => return adobe_error_reply(eyre!("Can't read body: {:?}", err)),
    };
    builder.body(body).into_response()
}

fn adobe_error_reply(err: Report) -> reply::Response {
    let message = format!("Malformed Adobe response: {}", err);
    error!("{}", &message);
    let body = json!({"statusCode": 500, "message": message});
    proxy_reply(http::StatusCode::INTERNAL_SERVER_ERROR, &body)
}

pub fn proxy_id() -> String {
    format!("adlu-proxy-{}", env!("CARGO_PKG_VERSION"))
}

pub fn proxy_via() -> String {
    format!("1.1 {}", proxy_id())
}
