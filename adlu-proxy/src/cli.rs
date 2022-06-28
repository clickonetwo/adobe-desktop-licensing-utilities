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

#[derive(Parser, Debug)]
#[clap(about = "A caching, store/forward, reverse proxy for Adobe FRL licensing")]
pub struct FrlProxy {
    #[clap(short, long, default_value = "proxy-conf.toml")]
    /// Path to config file.
    pub config_file: String,

    #[clap(short, parse(from_occurrences))]
    /// Specify once to force log level to debug.
    /// Specify twice to force log level to trace.
    pub debug: u8,

    #[clap(short, long)]
    /// Override configured log destination: 'console' or 'file'.
    /// You can use just the first letter, so '-l c' and '-l f' work.
    pub log_to: Option<String>,

    #[clap(subcommand)]
    pub cmd: Command,
}

#[derive(Parser, Debug)]
/// FRL Proxy
pub enum Command {
    /// Start the proxy server
    Start {
        #[clap(short, long)]
        /// Mode to run the proxy in, one of passthrough, cache, store, or forward.
        /// You can use any prefix of these names (minimally p, c, s, or f)
        mode: Option<String>,

        #[clap(long, parse(try_from_str))]
        /// Enable SSL? (true or false)
        ssl: Option<bool>,
    },
    /// Interactively create the config file
    Configure,
    /// Clear the cache (requires confirmation)
    Clear {
        #[clap(short, long)]
        /// Bypass confirmation prompt
        yes: bool,
    },
    /// Import stored responses from a forwarder
    Import { import_path: String },
    /// Export stored requests for a forwarder
    Export { export_path: String },
}
