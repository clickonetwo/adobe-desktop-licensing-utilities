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
use future::Future;
use std::future;
use std::sync::Mutex;

use log::{debug, error, info};

pub fn get_first_interrupt() -> impl Future<Output = ()> + Send + 'static {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let first_interrupt = async {
        match rx.await {
            Ok(_) => (),
            Err(_) => debug!("Interrupt sender has closed the channel"),
        }
    };
    let interrupt_sender = move || {
        if tx.send(()).is_err() {
            debug!("Interrupt receiver has closed the channel");
        }
    };
    let call_once = Mutex::new(Some(interrupt_sender));
    ctrlc::set_handler(move || {
        if let Some(f) = call_once.lock().unwrap().take() {
            info!("Caught initial interrupt");
            f();
        } else {
            error!("Caught subsequent interrupt");
        }
    })
    .unwrap();
    first_interrupt
}
