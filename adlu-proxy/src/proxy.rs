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
use eyre::{eyre, Context, Report, Result};
use log::{debug, error, info, warn};
use serde_json::{json, Value};
use warp::{Filter, Rejection, Reply};

use adlu_base::{load_pem_files, load_pfx_file, CertificateData, Timestamp};
pub use adlu_parse::protocol::{Request, RequestType};

use crate::cache::Cache;
use crate::settings::{ProxyMode, Settings};

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
        let mut builder = http::Response::builder().status(resp.status);
        if let Some(server) = resp.server {
            builder = builder.header("Server", server);
        }
        if let Some(via) = resp.via {
            builder = builder.header("Via", format!("{}, {}", via, proxy_id()));
        } else {
            builder = builder.header("Via", proxy_id());
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
        let timestamp = if let Some(val) = resp.headers().get("Date") {
            val.to_str().map(Timestamp::from_db).unwrap_or_default()
        } else {
            Timestamp::now()
        };
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
        let body = if content.is_empty() { None } else { Some(content) };
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

pub fn routes(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    status_route(conf.clone())
        .or(frl_activate_route(conf.clone()))
        .or(frl_deactivate_route(conf.clone()))
        .or(nul_license_route(conf.clone()))
        .or(upload_route(conf.clone()))
        .or(unknown_route(conf))
        .with(warp::log("route::summary"))
}

pub fn with_conf(
    conf: Config,
) -> impl Filter<Extract = (Config,), Error = std::convert::Infallible> + Clone {
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

pub fn frl_activate_route(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    Request::frl_activation_boxed_filter()
        .and(with_conf(conf))
        .then(process_adobe_request)
}

pub fn frl_deactivate_route(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    Request::frl_deactivation_boxed_filter()
        .and(with_conf(conf))
        .then(process_adobe_request)
}

pub fn nul_license_route(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    Request::nul_license_boxed_filter().and(with_conf(conf)).then(process_adobe_request)
}

pub fn upload_route(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    Request::log_upload_boxed_filter().and(with_conf(conf)).then(process_adobe_request)
}

pub fn unknown_route(
    conf: Config,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    // we only pass requests to Adobe if they are intended for an Adobe server
    to_adobe_host()
        .and(Request::unknown_boxed_filter())
        .and(with_conf(conf))
        .then(process_adobe_request)
        .recover(|err: Rejection| async move {
            if err.is_not_found() {
                info!("Rejecting unknown request to non-Adobe endpoint");
                let reply = serde_json::json!({"status": "Not Found", "statusCode": 404});
                Ok(proxy_reply(http::StatusCode::NOT_FOUND, &reply))
            } else {
                warn!("Unknown request rejected for unknown reason: {:?}", err);
                let message = format!("Request rejected: {:?}", err);
                let reply = serde_json::json!({"status": message, "statusCode": 500});
                Ok(proxy_reply(http::StatusCode::INTERNAL_SERVER_ERROR, &reply))
            }
        })
}

fn to_adobe_host() -> impl Filter<Extract = (), Error = Rejection> + Clone {
    warp::host::optional()
        .and_then(|auth: Option<http::uri::Authority>| async move {
            match auth {
                Some(auth) if auth.host().to_ascii_lowercase().contains(".adobe.") => {
                    Ok(())
                }
                _ => Err(warp::reject::not_found()),
            }
        })
        .untuple_one()
}

pub async fn status(conf: Config) -> warp::reply::Response {
    let status = format!("{} running in {:?} mode", proxy_id(), conf.settings.proxy.mode);
    info!("Status request received, issuing status: {}", &status);
    let body = json!({"statusCode": 200, "status": &status});
    proxy_reply(http::StatusCode::OK, &body)
}

pub async fn process_adobe_request(req: Request, conf: Config) -> warp::reply::Response {
    info!("Received {}", req);
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
    let outcome = if let ProxyMode::Isolated = conf.settings.proxy.mode {
        info!("Isolated - not forwarding {}", req);
        SendOutcome::Isolated
    } else {
        info!("Sending {} to Adobe endpoint", req);
        match send_to_adobe(req, conf).await {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    info!("Received valid response status for {}: {}", req, status);
                    match Response::from_network(req, response).await {
                        Ok(resp) => {
                            debug!("Response for {}: {:?}", req, resp);
                            // cache the response
                            conf.cache.store_response(req, &resp).await;
                            SendOutcome::Success(resp)
                        }
                        Err(err) => {
                            error!("Can't parse response for {}: {}", req, err);
                            SendOutcome::ParseFailure(err)
                        }
                    }
                } else {
                    info!("Received failure status for {}: {}", req, status);
                    debug!("Response for {}: {:?}", req, response);
                    // return the safe bits of the response
                    SendOutcome::ErrorStatus(response)
                }
            }
            Err(err) => {
                info!("Network failure sending {}", req);
                SendOutcome::Unreachable(err)
            }
        }
    };
    if let SendOutcome::Success(resp) = outcome {
        SendOutcome::Success(resp)
    } else if let Some(resp) = conf.cache.fetch_response(req).await {
        info!("Using previously cached response for {}", req);
        SendOutcome::Success(resp)
    } else {
        outcome
    }
}

pub async fn send_to_adobe(req: &Request, conf: &Config) -> Result<reqwest::Response> {
    let server = match req.request_type {
        RequestType::LogUpload => conf.log_server.as_str(),
        _ => conf.frl_server.as_str(),
    };
    let endpoint = if let Some(query) = &req.query {
        format!("{}/{}?{}", server, &req.path, query)
    } else {
        format!("{}/{}", server, &req.path)
    };
    let mut builder = conf
        .client
        .request(req.method.clone(), &endpoint)
        .header("Accept-Encoding", "gzip, deflate, br");
    if let Some(content_type) = &req.content_type {
        builder = builder.header("Content-Type", content_type)
    }
    if let Some(accept_type) = &req.accept_type {
        builder = builder.header("Accept", accept_type)
    }
    if let Some(accept_language) = &req.accept_language {
        builder = builder.header("Accept-Language", accept_language);
    }
    if let Some(api_key) = &req.api_key {
        builder = builder.header("X-Api-Key", api_key);
    }
    if let Some(request_id) = &req.request_id {
        builder = builder.header("X-Request-Id", request_id);
    }
    if let Some(session_id) = &req.session_id {
        builder = builder.header("X-Session-Id", session_id);
    }
    if let Some(authorization) = &req.authorization {
        builder = builder.header("Authorization", authorization);
    }
    if let Some(body) = &req.body {
        builder = builder.body(body.clone())
    }
    let request = builder.build().wrap_err("Error creating network request")?;
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

fn proxy_reply(status: http::StatusCode, body: &Value) -> warp::reply::Response {
    warp::reply::with_status(
        warp::reply::with_header(warp::reply::json(body), "Via", proxy_via()),
        status,
    )
    .into_response()
}

fn proxy_offline_reply() -> warp::reply::Response {
    let message = "Proxy is operating offline: request stored for later replay";
    debug!("{}", message);
    let body = json!({"statusCode": 502, "message": message});
    proxy_reply(http::StatusCode::BAD_GATEWAY, &body)
}

fn unreachable_reply(err: Report) -> warp::reply::Response {
    let message = format!("Could not reach Adobe: {}", err);
    error!("{}", &message);
    let body = json!({"statusCode": 502, "message": message});
    proxy_reply(http::StatusCode::BAD_GATEWAY, &body)
}

async fn adobe_bad_status_reply(resp: reqwest::Response) -> warp::reply::Response {
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

fn adobe_error_reply(err: Report) -> warp::reply::Response {
    let message = format!("Invalid Adobe response: {}", err);
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

#[cfg(test)]
mod tests {
    use super::to_adobe_host;
    use adlu_parse::protocol::Request;
    use warp::Filter;

    #[tokio::test]
    async fn unknown_request_accept_or_reject() {
        let filter = to_adobe_host().and(Request::unknown_boxed_filter());
        warp::test::request()
            .method("GET")
            .path("https://test.adobe.com/")
            .filter(&filter)
            .await
            .expect("Request to adobe server was rejected");
        warp::test::request()
            .method("GET")
            .path("https://test.clickonetwo.io/")
            .filter(&filter)
            .await
            .expect_err("Request to non-adobe server was accepted");
    }
}
