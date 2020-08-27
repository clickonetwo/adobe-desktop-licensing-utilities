#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub remote_host: String,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            host: "127.0.0.1:3030".to_owned(),
            remote_host: "http://localhost:3000".to_owned(),
        }
    }
}
