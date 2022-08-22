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
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct ProxyArgs {
    #[clap(short, long, default_value = "proxy-conf.toml")]
    /// Path to config file.
    pub config_file: String,

    #[clap(short, long, action = clap::ArgAction::Count)]
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

#[derive(Subcommand, Debug)]
/// Proxy commands
pub enum Command {
    /// Interactively create the config file
    Configure,
    /// Start the proxy server
    Serve {
        #[clap(short, long)]
        /// Handle requests in transparent, connected, or isolated mode.
        /// You can use any prefix of these names (minimally t, c, or i).
        /// Overrides the config file setting.
        mode: Option<String>,

        #[clap(long, parse(try_from_str))]
        /// Enable SSL? (true or false).
        /// Overrides the config file setting.
        ssl: Option<bool>,
    },
    /// Clear the cache (requires confirmation)
    Clear {
        #[clap(short, long)]
        /// Bypass confirmation prompt
        yes: bool,
    },
    /// Forward un-answered requests
    Forward,
    /// Import from other proxy's database
    Import { from_path: String },
    /// Export to other proxy's database
    Export { to_path: String },
    /// Report on database contents
    Report { to_path: String },
}
