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
use crate::settings::{LogDestination, LogLevel, Settings};
use eyre::{Result, WrapErr};
use fern::{log_file, Dispatch};
use log::LevelFilter;
use std::io;

pub fn init(conf: &Settings) -> Result<()> {
    let mut base_config = Dispatch::new().format(|out, message, record| {
        out.finish(format_args!(
            "{}[{}][{}] {}",
            chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
            record.target(),
            record.level(),
            message
        ))
    });
    let level = log_level(&conf.logging.level);
    match conf.logging.destination {
        LogDestination::Console => {
            base_config =
                base_config.chain(Dispatch::new().level(level).chain(io::stdout()))
        }
        LogDestination::File => {
            base_config = base_config.chain(
                Dispatch::new().level(level).chain(log_file(&conf.logging.file_path)?),
            )
        }
    }
    base_config.apply().wrap_err("Cannot initialize logging subsystem")?;
    Ok(())
}

fn log_level(level: &LogLevel) -> LevelFilter {
    match level {
        LogLevel::Off => LevelFilter::Off,
        LogLevel::Error => LevelFilter::Error,
        LogLevel::Warn => LevelFilter::Warn,
        LogLevel::Info => LevelFilter::Info,
        LogLevel::Debug => LevelFilter::Debug,
        LogLevel::Trace => LevelFilter::Trace,
    }
}
