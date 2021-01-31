/*
Copyright 2020 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
use crate::settings::{LogDestination, LogLevel, Settings};
use fern::{log_file, Dispatch, InitError};
use log::LevelFilter;
use std::io;

pub fn init(conf: &Settings) -> Result<(), InitError> {
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
    base_config.apply()?;
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
