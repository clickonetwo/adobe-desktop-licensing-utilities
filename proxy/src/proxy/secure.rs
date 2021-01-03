/*
Copyright 2020 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
use core::task::{Context, Poll};
use futures_util::stream::{Stream, StreamExt};
use hyper::server::Server;
use hyper::service::{make_service_fn, service_fn};
use log::{error, info};
use rustls::internal::pemfile;
use std::pin::Pin;
use std::vec::Vec;
use std::{fs, io, sync};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;

use crate::cache::Cache;
use crate::settings::Settings;

use super::{ctrl_c_handler, serve_req};
use std::sync::Arc;

fn error(err: String) -> io::Error {
    io::Error::new(io::ErrorKind::Other, err)
}

pub async fn run_server(
    conf: &Settings, cache: Arc<Cache>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Build TLS configuration.
    let tls_cfg = {
        // Load public certificate.
        let certs = load_certs(conf.proxy.ssl_cert.as_ref().unwrap())?;
        // Load private key.
        let key = load_private_key(conf.proxy.ssl_key.as_ref().unwrap())?;
        // Do not use client certificate authentication.
        let mut cfg = rustls::ServerConfig::new(rustls::NoClientAuth::new());
        // Select a certificate to use.
        cfg.set_single_cert(certs, key)
            .map_err(|e| error(format!("{}", e)))?;
        // Configure ALPN to accept HTTP/2, HTTP/1.1 in that order.
        cfg.set_protocols(&[b"h2".to_vec(), b"http/1.1".to_vec()]);
        sync::Arc::new(cfg)
    };

    // Create a TCP listener via tokio.
    let mut tcp = TcpListener::bind(&conf.proxy.host).await?;
    let tls_acceptor = &TlsAcceptor::from(tls_cfg);
    // Prepare a long-running future stream to accept and serve cients.
    let incoming_tls_stream = tcp
        .incoming()
        .filter_map(move |s| async move {
            let client = match s {
                Ok(x) => x,
                Err(e) => {
                    error!("Failed to accept client");
                    return Some(Err(e));
                }
            };
            match tls_acceptor.accept(client).await {
                Ok(x) => Some(Ok(x)),
                Err(e) => {
                    error!("[!] Voluntary server halt: {}", e);
                    None
                }
            }
        })
        .boxed();

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
    let server = Server::builder(HyperAcceptor {
        acceptor: incoming_tls_stream,
    })
    .serve(service);

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    ctrl_c_handler(move || tx.send(()).unwrap_or(()));

    let graceful = server.with_graceful_shutdown(async {
        rx.await.ok();
    });

    // Run the future, keep going until an error occurs.
    info!("Starting to serve on https://{}", conf.proxy.host);
    graceful.await?;
    Ok(())
}

struct HyperAcceptor<'a> {
    acceptor: Pin<Box<dyn Stream<Item = Result<TlsStream<TcpStream>, io::Error>> + 'a>>,
}

impl hyper::server::accept::Accept for HyperAcceptor<'_> {
    type Conn = TlsStream<TcpStream>;
    type Error = io::Error;

    fn poll_accept(
        mut self: Pin<&mut Self>, cx: &mut Context,
    ) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        Pin::new(&mut self.acceptor).poll_next(cx)
    }
}

// Load public certificate from file.
fn load_certs(filename: &str) -> io::Result<Vec<rustls::Certificate>> {
    // Open certificate file.
    let certfile = fs::File::open(filename)
        .map_err(|e| error(format!("failed to open {}: {}", filename, e)))?;
    let mut reader = io::BufReader::new(certfile);

    // Load and return certificate.
    pemfile::certs(&mut reader).map_err(|_| error("failed to load certificate".into()))
}

// Load private key from file.
fn load_private_key(filename: &str) -> io::Result<rustls::PrivateKey> {
    // Open keyfile.
    let keyfile = fs::File::open(filename)
        .map_err(|e| error(format!("failed to open {}: {}", filename, e)))?;
    let mut reader = io::BufReader::new(keyfile);

    // Load and return a single private key.
    // try rsa first
    let keys = pemfile::rsa_private_keys(&mut reader)
        .map_err(|_| error("failed to load private key".into()))?;
    if !keys.is_empty() {
        return Ok(keys[0].clone());
    }
    // if not try pkcs8
    let keys = pemfile::pkcs8_private_keys(&mut reader)
        .map_err(|_| error("failed to load private key".into()))?;
    if keys.len() != 1 {
        return Err(error("expected a single private key".into()));
    }
    Ok(keys[0].clone())
}
