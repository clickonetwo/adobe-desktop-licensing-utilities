use structopt::StructOpt;

#[derive(Debug, StructOpt)]
/// FRL Proxy
pub enum Opt {
    /// Start the proxy server
    Start {
        #[structopt(short, long)]
        /// Path to optional config file
        config_file: Option<String>,

        #[structopt(long)]
        /// Proxy hostname
        host: Option<String>,

        #[structopt(long)]
        /// Remote (licensing server) hostname
        remote_host: Option<String>,

        #[structopt(long)]
        /// Enable SSL
        ssl: bool,

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
    }
}
