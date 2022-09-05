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

use chrono::{DateTime, Local, TimeZone, Utc};
use eyre::{eyre, Result, WrapErr};
use log::warn;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

pub use certificate::{load_pem_files, load_pfx_file, CertificateData};
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use ngl::get_adobe_device_id;
pub use signal::get_first_interrupt;

mod certificate;
#[cfg(any(target_os = "macos", target_os = "windows"))]
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
    use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize, Serializer};
    // This module implements serialization and deserialization from
    // a JSON string.  It's intended for embedding JSON as
    // a field value inside of a template data structure

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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp {
    pub millis: i64,
}

impl Serialize for Timestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = self.to_string();
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl Timestamp {
    pub fn from_millis(epoch_millis: i64) -> Self {
        Self { millis: epoch_millis }
    }

    pub fn now() -> Self {
        Self { millis: Utc::now().timestamp_millis() }
    }

    pub fn to_millis(&self) -> i64 {
        self.millis
    }

    /// When you need to write a timestamp to an NGL log.
    pub fn to_log(&self) -> String {
        Utc.timestamp_millis(self.millis).format("%Y-%m-%dT%H:%M:%S:%3f%z").to_string()
    }

    /// When you've read a timestamp from a log and want it back.  This attempts to be
    /// tolerant of format changes, so if log date format changes it still works.
    pub fn from_log(s: &str) -> Self {
        match DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S:%3f%z") {
            Ok(ts) => Self::from_millis(ts.timestamp_millis()),
            Err(_) => {
                warn!("Unexpected time format in log: {}", s);
                Self::from_db(s)
            }
        }
    }

    /// When you need a date as an NGL device date in a request.
    pub fn to_device_date(&self) -> String {
        Local.timestamp_millis(self.millis).format("%Y-%m-%dT%H:%M:%S.%3f%z").to_string()
    }

    /// When you need a timestamp from an NGL device date in a request.
    pub fn from_device_date(s: &str) -> Self {
        match DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S.%3f%z") {
            Ok(ts) => Self::from_millis(ts.timestamp_millis()),
            Err(_) => {
                warn!("Unexpected time format in device date: {}", s);
                Self::from_db(s)
            }
        }
    }

    /// When you need to store a timestamp as a string field in a database.
    pub fn to_db(&self) -> String {
        self.to_string()
    }

    /// When you need to store an optional timestamp as a string field in a database.
    pub fn optional_to_db(t: &Option<Self>) -> String {
        match t {
            Some(timestamp) => timestamp.to_string(),
            None => String::new(),
        }
    }

    /// When you've stored a timestamp as a string and want it back.
    /// This handles both millisecond storage and various forms of date
    /// formatting, so it's backwards compatible with JSON storage
    /// prepared by different front ends.
    pub fn from_db(s: &str) -> Self {
        match s.parse::<Self>() {
            Ok(ts) => ts,
            Err(_) => Self::now(),
        }
    }

    /// When you've stored an optional timestamp as a string and want it back.
    pub fn optional_from_db(s: &str) -> Option<Self> {
        if s.is_empty() {
            None
        } else {
            Some(Self::from_db(s))
        }
    }
}

impl Default for Timestamp {
    fn default() -> Self {
        Self::now()
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Utc.timestamp_millis(self.millis)
            .format("%Y-%m-%dT%H:%M:%S%.3f%z")
            .to_string()
            .fmt(f)
    }
}

impl std::str::FromStr for Timestamp {
    type Err = chrono::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(val) = s.parse::<i64>() {
            // for now we assume milliseconds
            // TODO: figure out if it's seconds, milliseconds, or nanoseconds
            Ok(Self { millis: val })
        } else if let Ok(dt) = s.parse::<DateTime<Utc>>() {
            Ok(Self { millis: dt.timestamp_millis() })
        } else if let Ok(dt) = DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S:%3f%z") {
            Ok(Self { millis: dt.timestamp_millis() })
        } else if let Ok(dt) = DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S:%3f%:z") {
            Ok(Self { millis: dt.timestamp_millis() })
        } else if let Ok(dt) = DateTime::parse_from_rfc2822(s) {
            Ok(Self { millis: dt.timestamp_millis() })
        } else {
            match DateTime::parse_from_rfc3339(s) {
                Ok(dt) => Ok(Self { millis: dt.timestamp_millis() }),
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

#[cfg(target_os = "linux")]
pub fn get_saved_credential(_key: &str) -> Result<String> {
    Err(eyre!("Adobe caches credentials only on Mac and Win"))
}

#[cfg(test)]
mod tests {
    use super::Timestamp;

    #[test]
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn test_get_device_id() {
        let id = super::get_adobe_device_id();
        println!("The test machine's Adobe device ID is '{}'", id);
    }

    #[test]
    fn test_timestamp_from_storage() {
        let ts1 = Timestamp { millis: 0 };
        let ts2 = Timestamp::from_db("1970-01-01T00:00:00.000+00:00");
        assert_eq!(&ts1, &ts2);
        let ts3 = Timestamp::from_db("1970-01-01T00:00:00:000+00:00");
        assert_eq!(&ts1, &ts3);
        let ts4 = Timestamp::from_db("0000");
        assert_eq!(&ts1, &ts4);
    }
}
