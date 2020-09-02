pub mod plain;
pub mod secure;

use hyper::{Body, Client, Request, Response};
use futures::TryStreamExt;
use crate::settings::Settings;

async fn serve_req(req: Request<Body>, conf: Settings) -> Result<Response<Body>, hyper::Error> {
    let (parts, body) = req.into_parts();
    println!("received request at {:?}", parts.uri);
    println!("method {:?}", parts.method);
    println!("headers {:?}", parts.headers);
    let entire_body = body
        .try_fold(Vec::new(), |mut data, chunk| async move {
            data.extend_from_slice(&chunk);
            Ok(data)
        })
        .await?;
    println!("body {:?}", std::str::from_utf8(&entire_body).unwrap());
    // use the echo server for now
    let lcs_host = conf.proxy.remote_host;
    let url_str = match parts.uri.query() {
        Some(qstring) => format!("{}{}?{}", lcs_host, parts.uri.path(), qstring),
        None => format!("{}{}", lcs_host, parts.uri.path()),
    };
    let mut client_req_builder = Request::builder()
        .method(parts.method)
        .uri(url_str);
    for (k, v) in parts.headers.iter() {
        client_req_builder = client_req_builder.header(k, v);
    }
    let client_req = client_req_builder.body(Body::from(entire_body)).expect("error building client request");
    let res = Client::new().request(client_req).await?;
    Ok(res)
}
