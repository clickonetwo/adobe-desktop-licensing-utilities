/*
Copyright 2020 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
use structopt::StructOpt;

pub const DEFAULT_CONFIG_DIR: &str = if cfg!(target_os = "macos") {
    "/Library/Application Support/Adobe/OperatingConfigs"
} else if cfg!(target_os = "windows") {
    "${ProgramData}/Adobe/OperatingConfigs"
} else {
    "This module can only run on MacOS or Windows"
};

#[derive(Debug, StructOpt)]
/// Adobe License Decoder
///
/// Decodes all the installed license files on the current machine.
/// If you specify a directory, it will decode all the license files
/// or preconditioning files found in that directory.
pub struct Opt {
    /// Output additional license data (e.g., census codes)
    #[structopt(short, long)]
    pub verbose: bool,

    /// path to directory or file to decode
    pub path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::DEFAULT_CONFIG_DIR;
    use crate::utilities::FileInfo;

    #[test]
    fn test_os() {
        let config_path = String::from(DEFAULT_CONFIG_DIR);
        assert!(
            config_path.ends_with("/Adobe/OperatingConfigs"),
            "This module can only be compiled on Mac or Win"
        );
        let app_support_path = config_path.trim_end_matches("/Adobe/OperatingConfigs");
        assert!(
            FileInfo::from_path(app_support_path).is_ok(),
            "Application Support path is not present"
        );
    }
}
