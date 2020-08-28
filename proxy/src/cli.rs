use structopt::StructOpt;

#[derive(Debug, StructOpt)]
/// FRL Proxy
pub enum Opt {
    /// Start the proxy server
    Start {
        #[structopt(short, long)]
        config_file: Option<String>,
    },
    /// Create a template config file
    InitConfig {
        #[structopt(short, long, default_value = "config.toml")]
        out_file: String,
    }
}
