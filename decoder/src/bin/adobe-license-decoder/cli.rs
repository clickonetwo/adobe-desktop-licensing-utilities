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
use clap::Parser;

pub const DEFAULT_CONFIG_DIR: &str = if cfg!(target_os = "macos") {
    "/Library/Application Support/Adobe/OperatingConfigs"
} else if cfg!(target_os = "windows") {
    "${ProgramData}/Adobe/OperatingConfigs"
} else {
    "This module can only run on MacOS or Windows"
};

#[derive(Parser, Debug)]
/// Adobe License Decoder
///
/// Decodes all the installed license files on the current machine.
/// If you specify a directory, it will decode all the license files
/// or preconditioning files found in that directory.
pub struct Opt {
    /// Output additional data about each package (e.g., census codes).
    /// Specify this option more than once (-vv) to look in the credential
    /// store for any locally-cached application licenses.
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: i32,

    /// path to directory or file to decode
    #[clap(default_value = DEFAULT_CONFIG_DIR)]
    pub path: String,
}

#[cfg(test)]
mod tests {
    use super::DEFAULT_CONFIG_DIR;

    #[test]
    fn test_os() {
        let config_path = String::from(DEFAULT_CONFIG_DIR);
        assert!(
            config_path.ends_with("/Adobe/OperatingConfigs"),
            "This module can only be compiled on Mac or Win"
        );
        let app_support_path = config_path.trim_end_matches("/Adobe/OperatingConfigs");
        assert!(
            std::path::Path::new(app_support_path).is_dir(),
            "Application Support path is not present"
        );
    }
}
