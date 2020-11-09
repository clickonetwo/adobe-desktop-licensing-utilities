/*
 * MIT License
 *
 * Copyright (c) 2020 Adobe, Inc.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */
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
