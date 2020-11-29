/*
 * MIT License
 *
 * Copyright (c) 2020 Adobe, Inc.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */
use structopt::StructOpt;
use openssl_probe;

mod settings;
mod cli;
mod proxy;
mod logging;
mod cache;

use settings::Settings;
use cli::Opt;
use proxy::{plain, secure};

use log::debug;
use crate::cache::cache_control;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    openssl_probe::init_ssl_cert_env_vars();
    let opt = Opt::from_args();

    match opt {
        cli::Opt::Start { config_file, host, remote_host, ssl, ssl_cert, ssl_key } => {
            let conf = Settings::from_start(config_file, host, remote_host, ssl, ssl_cert, ssl_key)?;
            conf.validate()?;
            logging::init(&conf)?;
            debug!("conf: {:?}", conf);
            if let Some(true) = conf.proxy.ssl {
                secure::run_server(&conf).await?;
            } else {
                plain::run_server(&conf).await?;
            }
        }
        cli::Opt::InitConfig { out_file } => {
            settings::config_template(out_file)?;
            std::process::exit(0);
        }
        cli::Opt::CacheControl {
            config_file, clear, export_file, import_file
        } => {
            let conf = Settings::from_cache_control(config_file)?;
            conf.validate()?;
            let cache = cache::Cache::from_conf(&conf).await?;
            cache::cache_control(&cache, clear, export_file, import_file).await?;
            debug!("conf: {:?}", conf);
        }
    }
    Ok(())
}
