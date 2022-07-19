/*
Copyright 2022 Daniel Brotsky. All rights reserved.

All of the copyrighted work in this repository is licensed under the
GNU Affero General Public License, reproduced in the LICENSE-AGPL file.

Attribution:

Some source files in this repository are derived from files in two Adobe Open
Source projects: the Adobe License Decoder repository found at this URL:
    https://github.com/adobe/adobe-license-decoder.rs
and the FRL Online Proxy repository found at this URL:
    https://github.com/adobe/frl-online-proxy

The files in those original works are copyright 2022 Adobe and the use of those
materials in this work is permitted by the MIT license under which they were
released.  That license is reproduced here in the LICENSE-MIT file.
*/
use std::convert::Infallible;

use warp::Filter;

use adlu_parse::protocol::FrlRequest;

use crate::handlers;
use crate::settings::ProxyConfiguration;

pub fn with_conf(
    conf: ProxyConfiguration,
) -> impl Filter<Extract = (ProxyConfiguration,), Error = Infallible> + Clone {
    warp::any().map(move || conf.clone())
}

pub fn status_route(
    conf: ProxyConfiguration,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::get().and(warp::path("status")).and(with_conf(conf)).then(handlers::status)
}

pub fn activate_route(
    conf: ProxyConfiguration,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::post()
        .and(warp::path!("asnp" / "frl_connected" / "values" / "v2"))
        .and(FrlRequest::activation_filter())
        .and(with_conf(conf))
        .then(handlers::process_web_request)
}

pub fn deactivate_route(
    conf: ProxyConfiguration,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::delete()
        .and(warp::path!("asnp" / "frl_connected" / "v1"))
        .and(FrlRequest::deactivation_filter())
        .and(with_conf(conf))
        .then(handlers::process_web_request)
}
