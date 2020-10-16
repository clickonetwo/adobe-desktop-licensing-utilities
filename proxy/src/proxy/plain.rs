use hyper::{service::{make_service_fn, service_fn}, Server};
use std::net::SocketAddr;
use eyre::Result;
use log::{info, error};

use crate::settings::Settings;
use super::{serve_req, ctrlc_handler};

pub async fn run_server(conf: &Settings) -> Result<()> {
    let addr: SocketAddr = conf.proxy.host.parse()?;
    info!("Listening on http://{}", addr);
    let make_svc = make_service_fn(move |_| {
        let conf = conf.clone();
        async move {
            Ok::<_, hyper::Error>(service_fn(move |_req| {
                let conf = conf.clone();
                async move { serve_req(_req, conf).await }
            }))
        }
    });
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    ctrlc_handler(move || tx.send(()).unwrap_or(()));
    let server = Server::bind(&addr)
        .serve(make_svc);

    let graceful = server
        .with_graceful_shutdown(async {
            rx.await.ok();
        });

    if let Err(e) = graceful.await {
        error!("server error: {}", e);
    }

    Ok(())
}
