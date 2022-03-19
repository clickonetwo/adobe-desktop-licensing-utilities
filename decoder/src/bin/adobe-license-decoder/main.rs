/*
Copyright 2021 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
mod cli;

use cli::{Opt, DEFAULT_CONFIG_DIR};
use decoder::describe_configuration;
use frl_config::Configuration;
use structopt::StructOpt;

fn main() {
    let opt: Opt = Opt::from_args();
    if let Ok(config) = Configuration::from_path(&opt.path) {
        describe_configuration(&config, opt.verbose);
    } else {
        if opt.path.eq_ignore_ascii_case(DEFAULT_CONFIG_DIR) {
            eprintln!("Error: There are no licenses installed on this computer")
        } else {
            eprintln!("Error: No such directory: {}", &opt.path)
        }
        std::process::exit(1);
    };
}
