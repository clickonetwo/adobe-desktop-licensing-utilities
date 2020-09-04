use structopt::StructOpt;

mod settings;
mod cli;
mod proxy;
mod logging;

use settings::Settings;
use cli::Opt;
use proxy::{plain, secure};

use log::debug;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let opt = Opt::from_args();

    match opt {
        cli::Opt::Start { config_file, host, remote_host, ssl, ssl_cert, ssl_key } => {
            let conf = Settings::new(config_file, host, remote_host, ssl, ssl_cert, ssl_key)?;
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
    }
    Ok(())
}
