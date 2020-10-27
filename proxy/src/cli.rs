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

        #[structopt(long, parse(from_str = parse_bool))]
        /// Enable SSL? (yes or no)
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
    }
}

fn parse_bool(arg: &str) -> bool {
    match arg.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" => true,
        _ => false,
    }
}
