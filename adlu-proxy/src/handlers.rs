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
use eyre::{eyre, Report, Result, WrapErr};
use log::{debug, error, info};
use warp::{reply, Reply};

use adlu_parse::protocol::{FrlRequest, FrlResponse};

use crate::settings::{ProxyConfiguration, ProxyMode};

pub async fn status(conf: ProxyConfiguration) -> reply::Response {
    let status = format!("Proxy running in {:?} mode", conf.settings.proxy.mode);
    info!("Status request received, issuing status: {}", &status);
    let body =
        serde_json::json!({"statusCode": 200, "version": &agent(), "status": &status});
    proxy_reply(502, reply::json(&body))
}

pub async fn process_web_request(
    req: FrlRequest,
    conf: ProxyConfiguration,
) -> reply::Response {
    info!("Received activation request id: {}", req.request_id());
    debug!("Received activation request: {:?}", &req);
    conf.cache.store_request(&req).await;
    if let ProxyMode::Store = conf.settings.proxy.mode {
        debug!("Store mode - not contacting COPS");
        return proxy_offline_reply();
    }
    match send_frl_request(&conf, &req).await {
        FrlOutcome::Success(resp) => proxy_reply(200, resp),
        FrlOutcome::NetworkError(err) => cops_failure_reply(err),
        FrlOutcome::ParseFailure(err) => cops_error_reply(err),
        FrlOutcome::ErrorStatus(response) => cops_bad_status_reply(response).await,
    }
}

pub async fn forward_stored_request(conf: &ProxyConfiguration, req: &FrlRequest) -> bool {
    matches!(send_frl_request(conf, req).await, FrlOutcome::Success(_))
}

pub enum FrlOutcome {
    Success(FrlResponse),
    NetworkError(Report),
    ParseFailure(Report),
    ErrorStatus(reqwest::Response),
}

pub async fn send_frl_request(conf: &ProxyConfiguration, req: &FrlRequest) -> FrlOutcome {
    let id = req.request_id();
    info!("Sending activation request to COPS with request ID {}", id);
    let outcome = match send_to_adobe(conf, req).await {
        Ok(response) => {
            if response.status().is_success() {
                match FrlResponse::from_network(req, response).await {
                    Ok(resp) => {
                        info!("Received valid activation for request ID {}", id);
                        debug!(
                            "Received valid response for request ID {}: {:?}",
                            id, resp
                        );
                        // cache the response
                        conf.cache.store_response(req, &resp).await;
                        FrlOutcome::Success(resp)
                    }
                    Err(err) => {
                        error!(
                            "Received invalid response for request ID {}: {}",
                            id, err
                        );
                        FrlOutcome::ParseFailure(err)
                    }
                }
            } else {
                let status = response.status();
                error!("Received failure status for request ID {}: {:?}", id, status);
                debug!("Received failure response: {:?}", response);
                // return the safe bits of the response
                FrlOutcome::ErrorStatus(response)
            }
        }
        Err(err) => {
            info!("Network failure on activation call for request ID {}", id);
            FrlOutcome::NetworkError(err)
        }
    };
    if let FrlOutcome::Success(resp) = outcome {
        FrlOutcome::Success(resp)
    } else if let Some(resp) = conf.cache.fetch_response(req).await {
        info!("Using previously cached response for request ID {}", id);
        FrlOutcome::Success(resp)
    } else {
        outcome
    }
}

async fn send_to_adobe(
    conf: &ProxyConfiguration,
    req: &FrlRequest,
) -> Result<reqwest::Response> {
    let endpoint = match req {
        FrlRequest::Activation(_) => {
            format!("{}/{}", &conf.adobe_server, "asnp/frl_connected/values/v2")
        }
        FrlRequest::Deactivation(_) => {
            format!("{}/{}", &conf.adobe_server, "asnp/frl_connected/v1")
        }
    };
    let method = match req {
        FrlRequest::Activation(_) => http::Method::POST,
        FrlRequest::Deactivation(_) => http::Method::DELETE,
    };
    let builder = conf
        .client
        .request(method, endpoint)
        .header("User-Agent", agent())
        // .header("Accept-Encoding", "gzip, deflate, br")
        // .header("Accept", "application/json")
        ;
    let request = req
        .to_network(builder)
        .build()
        .wrap_err("Failure building FRL network request")?;
    conf.client.execute(request).await.wrap_err("Error executing FRL network request")
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

fn cops_failure_reply(err: Report) -> reply::Response {
    let message = format!("Could not reach Adobe: {}", err);
    error!("{}", &message);
    let body = serde_json::json!({"statusCode": 502, "message": message});
    proxy_reply(502, reply::json(&body))
}

async fn cops_bad_status_reply(resp: reqwest::Response) -> reply::Response {
    let mut builder =
        http::Response::builder().status(resp.status()).header("server", agent());
    if let Some(request_id) = resp.headers().get("X-Request-Id") {
        builder = builder.header("X-Request-Id", request_id)
    }
    if let Some(content_type) = resp.headers().get("Content-Type") {
        builder = builder.header("Content-Type", content_type)
    } else {
        return cops_error_reply(eyre!("Missing content type: {:?}", resp));
    }
    let body = match resp.bytes().await {
        Ok(val) => val,
        Err(err) => return cops_error_reply(eyre!("Can't read body: {:?}", err)),
    };
    builder.body(body).into_response()
}

fn cops_error_reply(err: Report) -> reply::Response {
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
