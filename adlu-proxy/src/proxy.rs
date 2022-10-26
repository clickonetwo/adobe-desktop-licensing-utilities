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
use std::collections::HashMap;
use std::convert::Infallible;

use eyre::{eyre, Report, Result, WrapErr};
use log::{debug, error, info};
use warp::{reply, Filter, Rejection, Reply};

use adlu_base::{load_pem_files, load_pfx_file, CertificateData};
use adlu_parse::protocol::{Request, Response};

use crate::cache::Cache;
use crate::settings::{ProxyMode, Settings};

#[derive(Debug, Clone)]
pub struct Config {
    pub settings: Settings,
    pub cache: Cache,
    pub client: reqwest::Client,
    pub frl_server: String,
    pub log_server: String,
}

impl Config {
    pub fn new(settings: Settings, cache: Cache) -> Result<Self> {
        let mut builder = reqwest::Client::builder();
        builder = builder.timeout(std::time::Duration::new(59, 0));
        if settings.upstream.use_proxy {
            let proxy_host = format!(
                "{}://{}:{}",
                settings.upstream.proxy_protocol,
                settings.upstream.proxy_host,
                settings.upstream.proxy_port
            );
            let mut proxy = reqwest::Proxy::https(&proxy_host)
                .wrap_err("Invalid proxy configuration")?;
            if settings.upstream.use_basic_auth {
                proxy = proxy.basic_auth(
                    &settings.upstream.proxy_username,
                    &settings.upstream.proxy_password,
                );
            }
            builder = builder.proxy(proxy)
        }
        let client = builder.build().wrap_err("Can't create proxy client")?;
        let frl_server: http::Uri =
            settings.frl.remote_host.parse().wrap_err("Invalid FRL endpoint")?;
        let log_server: http::Uri =
            settings.log.remote_host.parse().wrap_err("Invalid log endpoint")?;
        Ok(Config {
            settings,
            cache,
            client,
            frl_server: frl_server.to_string(),
            log_server: log_server.to_string(),
        })
    }

    #[cfg(test)]
    pub fn clone_with_mode(&self, mode: &ProxyMode) -> Self {
        let mut new_settings = self.settings.as_ref().clone();
        new_settings.proxy.mode = mode.clone();
        let mut new_config = self.clone();
        new_config.settings = Settings::new(new_settings);
        new_config
    }

    pub fn bind_addr(&self) -> Result<std::net::SocketAddr> {
        let proxy_addr = if self.settings.proxy.ssl {
            format!("{}:{}", self.settings.proxy.host, self.settings.proxy.ssl_port)
        } else {
            format!("{}:{}", self.settings.proxy.host, self.settings.proxy.port)
        };
        proxy_addr.parse().wrap_err("Invalid proxy host/port configuration")
    }

    pub fn cert_data(&self) -> Result<CertificateData> {
        if self.settings.proxy.ssl {
            load_cert_data(&self.settings).wrap_err("SSL configuration failure")
        } else {
            Err(eyre!("SSL is not enabled"))
        }
    }
}

fn load_cert_data(settings: &Settings) -> Result<CertificateData> {
    if settings.ssl.use_pfx {
        load_pfx_file(&settings.ssl.cert_path, &settings.ssl.password)
            .wrap_err("Failed to load PKCS12 data:")
    } else {
        let key_pass = match settings.ssl.password.as_str() {
            "" => None,
            p => Some(p),
        };
        load_pem_files(&settings.ssl.key_path, &settings.ssl.cert_path, key_pass)
            .wrap_err("Failed to load certificate and key files")
    }
}

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
    status_route(conf.clone())
        .or(frl_activate_route(conf.clone()))
        .or(frl_deactivate_route(conf.clone()))
        .or(nul_activate_route(conf.clone()))
        .or(upload_route(conf))
        .or(record_post_route())
        .or(record_delete_route())
        .with(warp::log("route::summary"))
}

pub fn with_conf(
    conf: Config,
) -> impl Filter<Extract = (Config,), Error = Infallible> + Clone {
    warp::any().map(move || conf.clone())
}

pub fn status_route(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::get().and(warp::path("status")).and(with_conf(conf)).then(status)
}

pub fn record_post_route() -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone
{
    warp::post()
        // .and(warp::path!("asnp" / "nud"))
        .and(warp::path::full())
        .and(warp::filters::header::headers_cloned())
        .and(warp::body::content_length_limit(32_000))
        .and(warp::body::json())
        .then(record_post)
}

pub fn record_delete_route(
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::delete()
        // .and(warp::path!("asnp" / "nud"))
        .and(warp::path::full())
        .and(warp::filters::header::headers_cloned())
        .and(warp::query())
        .then(record_delete)
}

pub fn frl_activate_route(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::post()
        .and(warp::path!("asnp" / "frl_connected" / "values" / "v2"))
        .and(Request::frl_activation_filter())
        .and(with_conf(conf))
        .then(process_web_request)
}

pub fn frl_deactivate_route(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::delete()
        .and(warp::path!("asnp" / "frl_connected" / "v1"))
        .and(Request::frl_deactivation_filter())
        .and(with_conf(conf))
        .then(process_web_request)
}

pub fn nul_activate_route(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::post()
        .and(warp::path!("asnp" / "nud" / "v4"))
        .and(Request::nul_activation_filter())
        .and(with_conf(conf))
        .then(process_web_request)
}

pub fn upload_route(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::post()
        .and(warp::path!("ulecs" / "v1"))
        .and(Request::log_upload_filter())
        .and(with_conf(conf))
        .then(process_web_request)
}

pub async fn status(conf: Config) -> reply::Response {
    let status = format!("Proxy running in {:?} mode", conf.settings.proxy.mode);
    info!("Status request received, issuing status: {}", &status);
    let body =
        serde_json::json!({"statusCode": 200, "version": &agent(), "status": &status});
    proxy_reply(200, reply::json(&body))
}

pub async fn record_post(
    path: warp::path::FullPath,
    headers: http::HeaderMap,
    body: serde_json::Value,
) -> reply::Response {
    let path = path.as_str().to_string();
    info!("Unrecognized POST request to path: {}", &path);
    debug!("Request headers are: {:?}", headers);
    debug!(
        "Request body is: {}",
        serde_json::to_string_pretty(&body).unwrap_or_else(|_| body.to_string())
    );
    proxy_offline_reply()
}

pub async fn record_delete(
    path: warp::path::FullPath,
    headers: http::HeaderMap,
    query: HashMap<String, String>,
) -> reply::Response {
    let path = path.as_str().to_string();
    info!("Unrecognized DELETE request to path: {}", &path);
    debug!("Request headers are: {:?}", headers);
    debug!("Query is: {:?}", query);
    proxy_offline_reply()
}

pub async fn process_web_request(req: Request, conf: Config) -> reply::Response {
    info!("Received request id: {}", req.request_id());
    debug!("Received request: {:?}", &req);
    if !matches!(conf.settings.proxy.mode, ProxyMode::Isolated) {
        conf.cache.store_request(&req).await;
    }
    match send_request(&conf, &req).await {
        SendOutcome::Success(resp) => proxy_reply(200, resp),
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
    let id = req.request_id();
    let outcome = if let ProxyMode::Isolated = conf.settings.proxy.mode {
        info!("Isolated - not forwarding request ID {}", id);
        SendOutcome::Isolated
    } else {
        info!("Sending request ID {} to Adobe endpoint", id);
        match send_to_adobe(conf, req).await {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    info!(
                        "Received valid response status for request ID {}: {}",
                        id, status
                    );
                    match Response::from_network(req, response).await {
                        Ok(resp) => {
                            debug!("Response for request ID {}: {:?}", id, resp);
                            // cache the response
                            conf.cache.store_response(req, &resp).await;
                            SendOutcome::Success(resp)
                        }
                        Err(err) => {
                            error!("Can't parse response for request ID {}: {}", id, err);
                            SendOutcome::ParseFailure(err)
                        }
                    }
                } else {
                    error!("Received failure status for request ID {}: {}", id, status);
                    debug!("Response for request ID {}: {:?}", id, response);
                    // return the safe bits of the response
                    SendOutcome::ErrorStatus(response)
                }
            }
            Err(err) => {
                info!("Network failure sending request ID {}", id);
                SendOutcome::Unreachable(err)
            }
        }
    };
    if let SendOutcome::Success(resp) = outcome {
        SendOutcome::Success(resp)
    } else if let Some(resp) = conf.cache.fetch_response(req).await {
        info!("Using previously cached response for request ID {}", id);
        SendOutcome::Success(resp)
    } else {
        outcome
    }
}

async fn send_to_adobe(conf: &Config, req: &Request) -> Result<reqwest::Response> {
    let method = match req {
        Request::FrlActivation(_) => http::Method::POST,
        Request::FrlDeactivation(_) => http::Method::DELETE,
        Request::NulActivation(_) => http::Method::POST,
        Request::LogUpload(_) => http::Method::POST,
    };
    let endpoint = match req {
        Request::FrlActivation(_) => {
            format!("{}/{}", &conf.frl_server, "asnp/frl_connected/values/v2")
        }
        Request::FrlDeactivation(_) => {
            format!("{}/{}", &conf.frl_server, "asnp/frl_connected/v1")
        }
        Request::NulActivation(_) => {
            format!("{}/{}", &conf.frl_server, "asnp/nud/v4")
        }
        Request::LogUpload(_) => {
            format!("{}/{}", &conf.log_server, "ulecs/v1")
        }
    };
    let response_type = match req {
        Request::FrlActivation(_) => "application/json",
        Request::FrlDeactivation(_) => "application/json",
        Request::NulActivation(_) => "application/json",
        Request::LogUpload(_) => "*/*",
    };
    let builder = conf
        .client
        .request(method, endpoint)
        .header("User-Agent", agent())
        .header("Accept-Encoding", "gzip, deflate, br")
        .header("Accept", response_type)
        .header("Accept-Language", "en-us");
    let request =
        req.to_network(builder).build().wrap_err("Failure building network request")?;
    if cfg!(test) {
        mock_adobe_server(conf, request).await.wrap_err("Error mocking network request")
    } else {
        conf.client.execute(request).await.wrap_err("Error executing network request")
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

fn proxy_reply(status_code: u16, core: impl Reply) -> reply::Response {
    reply::with_status(
        reply::with_header(
            reply::with_header(core, "content-type", "application/json;charset=UTF-8"),
            "server",
            agent(),
        ),
        http::StatusCode::from_u16(status_code).unwrap(),
    )
    .into_response()
}

fn proxy_offline_reply() -> reply::Response {
    let message = "Proxy is operating offline: request stored for later replay";
    debug!("{}", message);
    let body = serde_json::json!({"statusCode": 502, "message": message});
    proxy_reply(502, reply::json(&body))
}

fn unreachable_reply(err: Report) -> reply::Response {
    let message = format!("Could not reach Adobe: {}", err);
    error!("{}", &message);
    let body = serde_json::json!({"statusCode": 502, "message": message});
    proxy_reply(502, reply::json(&body))
}

async fn adobe_bad_status_reply(resp: reqwest::Response) -> reply::Response {
    let mut builder =
        http::Response::builder().status(resp.status()).header("server", agent());
    if let Some(request_id) = resp.headers().get("X-Request-Id") {
        builder = builder.header("X-Request-Id", request_id)
    }
    if let Some(content_type) = resp.headers().get("Content-Type") {
        builder = builder.header("Content-Type", content_type)
    } else {
        return adobe_error_reply(eyre!("Missing content type: {:?}", resp));
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
    let body = serde_json::json!({"statusCode": 502, "message": message});
    proxy_reply(502, reply::json(&body))
}

pub fn agent() -> String {
    format!(
        "ADLU-Proxy/{} ({}/{})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        sys_info::os_release().as_deref().unwrap_or("Unknown")
    )
}
