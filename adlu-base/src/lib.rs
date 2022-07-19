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
use std::collections::HashMap;

use chrono::prelude::*;
use eyre::{eyre, Result, WrapErr};
use serde_json::Value;

pub use certificate::{load_pem_files, load_pfx_file, CertificateData};
#[cfg(target_os = "macos")]
pub use ngl::get_adobe_device_id;
pub use signal::get_first_interrupt;

mod certificate;
#[cfg(target_os = "macos")]
mod ngl;
mod signal;

pub type JsonMap = HashMap<String, Value>;

pub fn u64decode(s: &str) -> Result<String> {
    let bytes = base64::decode_config(s, base64::URL_SAFE_NO_PAD)?;
    String::from_utf8(bytes).wrap_err("Illegal payload encoding")
}

pub fn u64encode(s: &str) -> Result<String> {
    Ok(base64::encode_config(s, base64::URL_SAFE_NO_PAD))
}

pub fn json_from_base64(s: &str) -> Result<JsonMap> {
    serde_json::from_str(&u64decode(s)?).wrap_err("Illegal payload data")
}

pub fn json_from_str(s: &str) -> Result<JsonMap> {
    serde_json::from_str(s).wrap_err("Illegal license data")
}

pub mod base64_encoded_json {
    use serde::{Deserializer, Serialize, Serializer};
    // This module implements serialization and deserialization from
    // base64-encoded JSON.  It's intended for embedding JSON as
    // a field value inside of a larger data structure, but it can
    // be used at top-level if, for example, your transmission
    // medium can only handle ASCII strings.  The base64 encoding
    // used is URL-safe and un-padded, so you can also use this to
    // encode JSON in query strings.
    use serde::de::DeserializeOwned;
    use serde::Deserialize;

    pub fn serialize<S, T>(val: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        let json_str = serde_json::to_string(val).map_err(|e| {
            serde::ser::Error::custom(format!("Can't serialize into JSON: {:?}", e))
        })?;
        let base64_str = base64::encode_config(&json_str, base64::URL_SAFE_NO_PAD);
        serializer.serialize_str(&base64_str)
    }

    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: DeserializeOwned,
    {
        let base64_string = String::deserialize(deserializer)?;
        // println!("base64 string starts: {:?}", &base64_string);
        let json_bytes = base64::decode_config(&base64_string, base64::URL_SAFE_NO_PAD)
            .map_err(|e| {
            serde::de::Error::custom(&format!("Illegal base64: {:?}", e))
        })?;
        // println!("JSON bytes start: {:?}", &json_bytes);
        serde_json::from_reader(json_bytes.as_slice()).map_err(|e| {
            println!("Failure to parse looking for: {:?}", std::any::type_name::<T>());
            println!("JSON is: {}", &super::u64decode(&base64_string).unwrap());
            serde::de::Error::custom(&format!("Can't deserialize from JSON: {:?}", e))
        })
    }
}

pub mod template_json {
    use serde::{Deserializer, Serialize, Serializer};
    // This module implements serialization and deserialization from
    // a JSON string.  It's intended for embedding JSON as
    // a field value inside of a template data structure
    use serde::de::DeserializeOwned;
    use serde::Deserialize;

    pub fn serialize<S, T>(val: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        let json_str = serde_json::to_string(val).map_err(|e| {
            serde::ser::Error::custom(format!("Can't serialize into JSON: {:?}", e))
        })?;
        serializer.serialize_str(&json_str)
    }

    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: DeserializeOwned,
    {
        let json_string = String::deserialize(deserializer)?;
        serde_json::from_str(&json_string).map_err(|e| {
            serde::de::Error::custom(&format!("Can't deserialize from JSON: {:?}", e))
        })
    }
}

pub fn json_from_file(path: &str) -> Result<JsonMap> {
    let file = std::fs::File::open(std::path::Path::new(path))
        .wrap_err("Can't read license file")?;
    serde_json::from_reader(&file).wrap_err("Can't parse license data")
}

/// NGL timestamps are typically in millseconds since the Unix Epoch.
/// We have a type that holds them and allows conversion to and from
/// various integer and string forms, including datestamps.
/// The debug representation is a datestamp, while the string version
/// is the integer.  Either are acceptable for parsing.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(i64);

impl Timestamp {
    pub fn from_millis(epoch_millis: i64) -> Self {
        Timestamp(epoch_millis)
    }

    pub fn from_date(date_string: &str) -> Self {
        Self::from_storage(date_string)
    }

    pub fn now() -> Self {
        Self(Utc::now().timestamp_millis())
    }

    /// When you need to store a timestamp as a string,
    /// use this function.
    pub fn to_storage(&self) -> String {
        self.to_string()
    }

    /// An infallible parse, used for reading
    /// timestamps that have been stored as strings
    /// either in integer or datestamp form.
    pub fn from_storage(s: &str) -> Self {
        if let Ok(ts) = s.parse() {
            ts
        } else {
            Timestamp(Utc::now().timestamp_millis())
        }
    }
}

impl std::fmt::Debug for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Local.timestamp_millis(self.0).format("%Y-%m-%dT%H:%M:%S%.3f%z").fmt(f)
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for Timestamp {
    type Err = chrono::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(val) = s.parse::<i64>() {
            Ok(Timestamp(val))
        } else {
            match s.parse::<DateTime<Utc>>() {
                Ok(dt) => Ok(Timestamp(dt.timestamp_millis())),
                Err(err) => Err(err),
            }
        }
    }
}

pub fn local_date_from_epoch_millis(timestamp: &str) -> Result<String> {
    let timestamp = timestamp.parse::<i64>().wrap_err("Illegal license timestamp")?;
    let date = Local.timestamp_millis(timestamp);
    Ok(date.format("%Y-%m-%d").to_string())
}

#[cfg(target_os = "macos")]
pub fn get_saved_credential(key: &str) -> Result<String> {
    let service = format!("Adobe App Info ({})", &key);
    let entry = keyring::Entry::new(&service, "App Info");
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

#[cfg(test)]
mod tests {
    #[test]
    fn test_get_device_id() {
        let id = super::get_adobe_device_id();
        println!("The test machine's Adobe device ID is '{}'", id);
    }
}
