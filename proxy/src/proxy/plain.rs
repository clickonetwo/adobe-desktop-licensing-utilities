/*
Copyright 2020 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
use eyre::{Result, WrapErr};
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
            Ok::<_, hyper::Error>(service_fn(move |_req| {
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
