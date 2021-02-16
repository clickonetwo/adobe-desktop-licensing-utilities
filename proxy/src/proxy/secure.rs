/*
Copyright 2020 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
use super::{ctrl_c_handler, serve_req};
use crate::cache::Cache;
use crate::settings::Settings;
use async_stream::stream;
use core::task::{Context, Poll};
use eyre::{Result, WrapErr};
use futures_util::stream::{Stream, StreamExt};
use hyper::server::Server;
use hyper::service::{make_service_fn, service_fn};
use log::{error, info};
use std::fs::File;
use std::io::Read;
use std::pin::Pin;
use std::sync::Arc;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio_native_tls::{native_tls, TlsAcceptor, TlsStream};

pub async fn run_server(conf: &Settings, cache: Arc<Cache>) -> Result<()> {
    let acceptor = {
        let path = &conf.ssl.cert_path;
        let password = &conf.ssl.cert_password;
        let mut file = File::open(path)
            .wrap_err(format!("Can't open SSL certificate file: {}", path))?;
        let mut identity = vec![];
        file.read_to_end(&mut identity)
            .wrap_err(format!("Can't read SSL cert data from file: {}", path))?;
        let identity = native_tls::Identity::from_pkcs12(&identity, password)
            .wrap_err("Can't decrypt SSL cert data - incorrect password?")?;
        let sync_acceptor = native_tls::TlsAcceptor::new(identity)
            .wrap_err("Can't create TLS socket listener - is the port free?")?;
        let async_acceptor: TlsAcceptor = sync_acceptor.into();
        async_acceptor
    };
    let tcp = TcpListener::bind(&conf.proxy.host).await?;
    let incoming_tls_stream = incoming(tcp, acceptor).boxed();
    let hyper_acceptor = HyperAcceptor { acceptor: incoming_tls_stream };
    let service = make_service_fn(move |_| {
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
    let server = Server::builder(hyper_acceptor).serve(service);

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    ctrl_c_handler(move || tx.send(()).unwrap_or(()));
    let graceful = server.with_graceful_shutdown(async {
        rx.await.ok();
    });

    // Run the server, keep going until an error occurs.
    info!("Starting to serve on https://{}", conf.proxy.host);
    graceful.await.wrap_err("Unexpected server shutdown")?;
    Ok(())
}

fn incoming(
    listener: TcpListener, acceptor: TlsAcceptor,
) -> impl Stream<Item = TlsStream<TcpStream>> {
    stream! {
        loop {
            // just swallow errors and wait again if necessary
            match listener.accept().await {
                Ok((stream, _)) => {
                    match acceptor.accept(stream).await {
                        Ok(x) => { yield x; }
                        Err(e) => { error!("SSL Failure with client: {}", e); }
                    }
                }
                Err(e) => { error!("Connection failure with client: {}", e); }
            }
        };
    }
}

struct HyperAcceptor<'a> {
    acceptor: Pin<Box<dyn Stream<Item = TlsStream<TcpStream>> + 'a>>,
}

impl hyper::server::accept::Accept for HyperAcceptor<'_> {
    type Conn = TlsStream<TcpStream>;
    type Error = io::Error;

    fn poll_accept(
        mut self: Pin<&mut Self>, cx: &mut Context,
    ) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        let result = Pin::new(&mut self.acceptor).poll_next(cx);
        match result {
            Poll::Ready(Some(stream)) => Poll::Ready(Some(Ok(stream))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
