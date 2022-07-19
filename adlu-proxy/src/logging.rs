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
use eyre::{Result, WrapErr};
use log::LevelFilter;
use log4rs::{
    append::{
        console::{ConsoleAppender, Target},
        file::FileAppender,
    },
    config::{Appender, Config, Root},
    encode::pattern::PatternEncoder,
};

use crate::settings::{LogDestination, LogLevel, Settings};

pub fn init(settings: &Settings) -> Result<()> {
    let pattern = "{d([%Y-%m-%d][%H:%M:%S])}[{t}][{l}] {m}{n}";
    let encoder = PatternEncoder::new(pattern);
    let filter = log_level(&settings.logging.level);
    let appender = if let LogDestination::Console = &settings.logging.destination {
        Appender::builder().build(
            "logger",
            Box::new(
                ConsoleAppender::builder()
                    .encoder(Box::new(encoder))
                    .target(Target::Stdout)
                    .build(),
            ),
        )
    } else {
        Appender::builder().build(
            "logger",
            Box::new(
                FileAppender::builder()
                    .encoder(Box::new(encoder))
                    .build(&settings.logging.file_path)
                    .wrap_err("Can't create log file configuration")?,
            ),
        )
    };
    let config = Config::builder()
        .appender(appender)
        .build(Root::builder().appender("logger").build(filter))
        .wrap_err("Can't create root logging configuration")?;
    log4rs::init_config(config).wrap_err("Can't initialize logging")?;
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
