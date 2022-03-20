/*
Copyright 2020 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(about = "A caching, store/forward, reverse proxy for Adobe FRL licensing")]
pub struct FrlProxy {
    #[structopt(short, long, default_value = "proxy-conf.toml")]
    /// Path to config file.
    pub config_file: String,

    #[structopt(short, parse(from_occurrences))]
    /// Specify once to force log level to debug.
    /// Specify twice to force log level to trace.
    pub debug: u8,

    #[structopt(short, long)]
    /// Override configured log destination: 'console' or 'file'.
    /// You can use just the first letter, so '-l c' and '-l f' work.
    pub log_to: Option<String>,

    #[structopt(subcommand)]
    pub cmd: Command,
}

#[derive(Debug, StructOpt)]
/// FRL Proxy
pub enum Command {
    /// Start the proxy server
    Start {
        #[structopt(short, long)]
        /// Mode to run the proxy in, one of passthrough, cache, store, or forward.
        /// You can use any prefix of these names (minimally p, c, s, or f)
        mode: Option<String>,

        #[structopt(long, parse(try_from_str))]
        /// Enable SSL? (true or false)
        ssl: Option<bool>,
    },
    /// Interactively create the config file
    Configure,
    /// Clear the cache (requires confirmation)
    Clear {
        #[structopt(short, long)]
        /// Bypass confirmation prompt
        yes: bool,
    },
    /// Import stored responses from a forwarder
    Import { import_path: String },
    /// Export stored requests for a forwarder
    Export { export_path: String },
}
