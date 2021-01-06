/*
Copyright 2020 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
use eyre::Result;
use hyper::Server;
use hyper::service::{make_service_fn, service_fn};
use log::{error, info};
use std::net::SocketAddr;
use std::sync::Arc;

use super::{ctrl_c_handler, serve_req};
use crate::cache::Cache;
use crate::settings::Settings;

pub async fn run_server(conf: &Settings, cache: Arc<Cache>) -> Result<()> {
    let addr: SocketAddr = conf.proxy.host.parse()?;
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

    if let Err(e) = graceful.await {
        error!("server error: {}", e);
    }

    Ok(())
}
