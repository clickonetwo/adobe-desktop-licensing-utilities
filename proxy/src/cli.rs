/*
Copyright 2020 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
use std::str::ParseBoolError;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
/// FRL Proxy
pub enum Opt {
    /// Start the proxy server
    Start {
        #[structopt(short, long)]
        /// Path to optional config file
        config_file: Option<String>,

        #[structopt(short, long)]
        /// Mode to run the proxy in
        mode: Option<String>,

        #[structopt(long)]
        /// Proxy hostname
        host: Option<String>,

        #[structopt(long)]
        /// Remote (licensing server) hostname
        remote_host: Option<String>,

        #[structopt(long, parse(try_from_str = parse_bool))]
        /// Enable SSL? (true or false)
        ssl: Option<bool>,

        #[structopt(long)]
        /// Path to SSL certificate
        ssl_cert: Option<String>,

        #[structopt(long)]
        /// Path to SSL private key
        ssl_key: Option<String>,
    },
    /// Create a template config file
    InitConfig {
        #[structopt(short, long, default_value = "config.toml")]
        /// path to config filename
        out_file: String,
    },
    /// Manage the cache file
    CacheControl {
        #[structopt(short, long)]
        /// Path to optional config file
        config_file: Option<String>,

        #[structopt(long, parse(try_from_str = parse_bool))]
        /// Whether to clear the cache (dangerous!)
        clear: Option<bool>,

        #[structopt(short, long)]
        export_file: Option<String>,

        #[structopt(short, long)]
        import_file: Option<String>,
    },
}

fn parse_bool(arg: &str) -> Result<bool, ParseBoolError> {
    match arg.to_ascii_lowercase().as_str() {
        "1" | "yes" => Ok(true),
        "0" | "no" => Ok(false),
        arg => arg.parse(),
    }
}
