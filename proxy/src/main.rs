use hyper::{service::{make_service_fn, service_fn}, Body, Client, Request,
            Response, Server};
use std::net::SocketAddr;

async fn serve_req(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    println!("received request at {:?}", req.uri());
    println!("method {:?}", req.method());
    println!("headers {:?}", req.headers());
    // use the echo server for now
    let url_str = "http://localhost:3000";
    let mut client_req_builder = Request::builder()
        .method(req.method())
        .uri(url_str);
    for (k, v) in req.headers().iter() {
        // the hyper client we invoke later will set the host, so we 
        // don't want to set that here
        if k == "host" {
            continue;
        }
        client_req_builder = client_req_builder.header(k, v);
    }
    let client_req = client_req_builder.body(req.into_body()).expect("error building client request");
    let res = Client::new().request(client_req).await?;
    Ok(res)
}

async fn run_server(addr: SocketAddr) {
    println!("Listening on http://{}", addr);
    let serve_future = Server::bind(&addr)
        .serve(make_service_fn(|_| async {
            Ok::<_, hyper::Error>(service_fn(serve_req))
        }));
    if let Err(e) = serve_future.await {
        eprintln!("server error: {}", e);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = "127.0.0.1:3030".parse::<SocketAddr>()?;
    run_server(addr).await;
    Ok(())
}
