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
use log::{error, info};
use warp::Filter;

use adlu_base::{get_first_interrupt, load_pfx_file};

use crate::api::{activate_route, deactivate_route, status_route};
use crate::cache::Cache;
use crate::handlers;
use crate::settings::{ProxyConfiguration, Settings};

pub async fn serve_incoming_https_requests(settings: Settings, cache: Cache) {
    let conf = ProxyConfiguration::new(&settings, &cache);
    let routes = activate_route(conf.clone())
        .or(deactivate_route(conf.clone()))
        .or(status_route(conf.clone()));
    let pfx_data = load_pfx_file(&settings.ssl.cert_path, &settings.ssl.cert_password)
        .unwrap_or_else(|err| panic!("Failed to load certificate data: {}", err));
    let (addr, server) = warp::serve(routes)
        .tls()
        .cert(pfx_data.cert_pem())
        .key(pfx_data.key_pem())
        .bind_with_graceful_shutdown(conf.bind_addr, get_first_interrupt());
    info!("Serving HTTPS requests on {:?}...", addr);
    match tokio::task::spawn(server).await {
        Ok(_) => info!("HTTPS server terminated normally"),
        Err(err) => error!("HTTPS server terminated abnormally: {:?}", err),
    }
}

pub async fn serve_incoming_http_requests(settings: Settings, cache: Cache) {
    let conf = ProxyConfiguration::new(&settings, &cache);
    let routes = activate_route(conf.clone())
        .or(deactivate_route(conf.clone()))
        .or(status_route(conf.clone()));
    let (addr, server) = warp::serve(routes)
        .bind_with_graceful_shutdown(conf.bind_addr, get_first_interrupt());
    info!("Serving HTTP requests on {:?}...", addr);
    match tokio::task::spawn(server).await {
        Ok(_) => info!("HTTP server terminated normally"),
        Err(err) => error!("HTTPS server terminated abnormally: {:?}", err),
    }
}

pub async fn forward_stored_requests(settings: Settings, cache: Cache) {
    let conf = ProxyConfiguration::new(&settings, &cache);
    let reqs = cache.fetch_forwarding_requests().await;
    if reqs.is_empty() {
        eprintln!("No requests to forward.");
        return;
    }
    let count = reqs.len();
    eprintln!("Found {} request(s) to forward", count);
    let (mut successes, mut failures) = (0u64, 0u64);
    for req in reqs.iter() {
        if handlers::forward_stored_request(&conf, req).await {
            successes += 1
        } else {
            failures += 1
        }
    }
    eprintln!(
        "Forwarding produced {} success(es) and {} failure(s).",
        successes, failures
    );
}
