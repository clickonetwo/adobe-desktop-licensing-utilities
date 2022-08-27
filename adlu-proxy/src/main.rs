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
use clap::Parser;

use adlu_base::get_first_interrupt;
use adlu_proxy::cli::ProxyArgs;
use adlu_proxy::settings;

#[tokio::main]
async fn main() {
    let args: ProxyArgs = ProxyArgs::parse();
    // if we have a valid config, proceed, else update the config
    if let Ok(settings) = settings::load_config_file(&args) {
        let stop_signal = get_first_interrupt();
        if let Err(err) = adlu_proxy::run(settings, args, stop_signal).await {
            eprintln!("Proxy failure: {}", err);
            std::process::exit(1);
        }
    } else {
        eprintln!("Couldn't read the configuration file, creating a new one...");
        if let Err(err) = settings::update_config_file(None, &args.config_file) {
            eprintln!("Failed to create config file: {}", err);
            std::process::exit(1);
        }
    }
}
