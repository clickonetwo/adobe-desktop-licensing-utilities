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
mod cli;
mod description;

use adlu_parse::admin::Configuration;
use clap::Parser;
use cli::{Opt, DEFAULT_CONFIG_DIR};
use description::describe_configuration;

fn main() {
    let opt: Opt = Opt::parse();
    match Configuration::from_path(&opt.path) {
        Ok(config) => describe_configuration(&config, opt.verbose),
        Err(err) => {
            if opt.path.eq_ignore_ascii_case(DEFAULT_CONFIG_DIR) {
                eprintln!("Error: There are no licenses installed on this computer")
            } else {
                eprintln!("Error: {}", err)
            }
            std::process::exit(1);
        }
    };
}
