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
use eyre::{eyre, Result};

#[cfg(target_os = "macos")]
pub fn get_saved_credential(key: &str) -> Result<String> {
    let service = format!("Adobe App Info ({})", &key);
    let entry = keyring::Entry::new(&service, "App Info")?;
    match entry.get_password() {
        Ok(s) => Ok(s),
        Err(keyring::Error::NoStorageAccess(err)) => {
            eprintln!("Credential store could not be accessed.  Is it unlocked?");
            Err(eyre!(err))
        }
        Err(err) => Err(eyre!(err)),
    }
}

#[cfg(target_os = "windows")]
pub fn get_saved_credential(key: &str) -> Result<String> {
    let mut result = String::new();
    for i in 1..100 {
        let service = format!("Adobe App Info ({})(Part{})", key, i);
        let entry = keyring::Entry::new_with_target(&service, &service, "App Info");
        match entry.get_password() {
            Ok(val) => result.push_str(val.trim()),
            Err(keyring::Error::NoStorageAccess(_)) => {
                eprintln!("Credential store could not be accessed.  Is it unlocked?");
                break;
            }
            Err(_) => break,
        }
    }
    if result.is_empty() {
        Err(eyre!("No credential data found"))
    } else {
        Ok(result)
    }
}

#[cfg(target_os = "linux")]
pub fn get_saved_credential(_key: &str) -> Result<String> {
    Err(eyre!("No credential data cached on Linux"))
}
