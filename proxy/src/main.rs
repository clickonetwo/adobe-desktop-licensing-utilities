use hyper::{service::{make_service_fn, service_fn}, Body, Client, Request,
            Response, Server, Uri};
use std::net::SocketAddr;

async fn serve_req(_req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    Ok(Response::new(Body::from("proxy responding")))
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
async fn main() {
    let addr = "127.0.0.1:3030".parse::<SocketAddr>().unwrap();
    run_server(addr).await;
}
