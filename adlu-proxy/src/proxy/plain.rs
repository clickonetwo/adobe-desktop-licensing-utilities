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
use eyre::{Report, Result, WrapErr};
use hyper::service::{make_service_fn, service_fn};
use hyper::Server;
use log::info;
use std::net::SocketAddr;
use std::sync::Arc;

use super::{ctrl_c_handler, serve_req};
use crate::cache::Cache;
use crate::settings::Settings;

pub async fn run_server(conf: &Settings, cache: Arc<Cache>) -> Result<()> {
    let full_host = format!("{}:{}", conf.proxy.host, conf.proxy.port);
    let addr: SocketAddr = full_host.parse()?;
    info!("Listening on http://{}", addr);
    let make_svc = make_service_fn(move |_| {
        let conf = conf.clone();
        let cache = Arc::clone(&cache);
        async move {
            Ok::<_, Report>(service_fn(move |_req| {
                let conf = conf.clone();
                let cache = Arc::clone(&cache);
                async move { serve_req(_req, conf, cache).await }
            }))
        }
    });
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    ctrl_c_handler(move || tx.send(()).unwrap_or(()));
    let server = Server::bind(&addr).serve(make_svc);

    let graceful = server.with_graceful_shutdown(async {
        rx.await.ok();
    });

    // Run the server, keep going until an error occurs.
    info!("Starting to serve on http://{}", full_host);
    graceful.await.wrap_err("Unexpected server shutdown")?;
    Ok(())
}
