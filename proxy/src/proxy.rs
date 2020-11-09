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
pub mod plain;
pub mod secure;

use hyper::{Body, Client, Request, Response, Uri};
use hyper_tls::HttpsConnector;
use futures::TryStreamExt;
use log::{info, debug};
use std::sync::Mutex;

use crate::settings::Settings;

fn ctrlc_handler<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    let call_once = Mutex::new(Some(f));

    ctrlc::set_handler(move || {
        if let Some(f) = call_once.lock().unwrap().take() {
            info!("Starting graceful shutdown");
            f();
        } else {
            info!("Already sent signal to start graceful shutdown");
        }
    })
    .unwrap();
}

async fn get_entire_body(body: Body) -> Result<Vec<u8>, hyper::Error> {
    body
        .try_fold(Vec::new(), |mut data, chunk| async move {
            data.extend_from_slice(&chunk);
            Ok(data)
        })
        .await
}

async fn serve_req(req: Request<Body>, conf: Settings) -> Result<Response<Body>, hyper::Error> {
    let (parts, body) = req.into_parts();
    info!("received request at {:?}", parts.uri);
    debug!("REQ method {:?}", parts.method);
    debug!("REQ headers {:?}", parts.headers);

    let entire_body = get_entire_body(body).await?;
    debug!("REQ body {:?}", std::str::from_utf8(&entire_body).unwrap());
    // use the echo server for now
    let lcs_uri = conf.proxy.remote_host.parse::<Uri>().unwrap_or_else(|_| panic!("failed to parse uri: {}", conf.proxy.remote_host));

    // if no scheme is specified for remote_host, assume http
    let lcs_scheme = match lcs_uri.scheme_str() {
        Some("https") => "https",
        _ => "http",
    };

    let lcs_host = match lcs_uri.port() {
        Some(port) => {
            let h = lcs_uri.host().unwrap();
            format!("{}:{}", h, port.as_str())
        },
        None => String::from(lcs_uri.host().unwrap())
    };

    let url_str = match parts.uri.query() {
        Some(qstring) => format!("{}://{}{}?{}", lcs_scheme, lcs_host, parts.uri.path(), qstring),
        None => format!("{}://{}{}", lcs_scheme, lcs_host, parts.uri.path()),
    };

    debug!("REQ URI {}", url_str);

    let mut client_req_builder = Request::builder()
        .method(parts.method)
        .uri(url_str);
    for (k, v) in parts.headers.iter() {
        if k == "host" {
            client_req_builder = client_req_builder.header(k, lcs_host.clone());
        } else {
            client_req_builder = client_req_builder.header(k, v);
        }
    }
    let client_req = client_req_builder.body(Body::from(entire_body)).expect("error building client request");

    let https = HttpsConnector::new();
    let res = if lcs_scheme == "https" {
        let client = Client::builder().build::<_, hyper::Body>(https);
        client.request(client_req).await?
    } else {
        Client::new().request(client_req).await?
    };

    let (parts, body) = res.into_parts();
    debug!("RES code {:?}", parts.status);
    debug!("RES headers {:?}", parts.headers);

    let entire_body = get_entire_body(body).await?;
    debug!("RES body {:?}", std::str::from_utf8(&entire_body).unwrap());
    Ok(Response::from_parts(parts, Body::from(entire_body)))
}
