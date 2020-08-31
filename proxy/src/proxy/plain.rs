use hyper::{service::{make_service_fn, service_fn}, Server};
use std::net::SocketAddr;
use eyre::Result;

use crate::settings::Settings;
use super::serve_req;

pub async fn run_server(conf: &Settings) -> Result<()> {
    let addr: SocketAddr = conf.proxy.host.parse()?;
    println!("Listening on http://{}", addr);
    let make_svc = make_service_fn(move |_| {
        let conf = conf.clone();
        async move {
            Ok::<_, hyper::Error>(service_fn(move |_req| {
                let conf = conf.clone();
                async move { serve_req(_req, conf).await }
            }))
        }
    });
    let serve_future = Server::bind(&addr)
        .serve(make_svc);
    if let Err(e) = serve_future.await {
        eprintln!("server error: {}", e);
    }
    Ok(())
}
