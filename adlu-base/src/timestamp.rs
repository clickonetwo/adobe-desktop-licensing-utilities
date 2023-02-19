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
use chrono::{DateTime, Local, LocalResult, TimeZone, Utc};
use eyre::Result;
use log::warn;
use serde::{Deserialize, Deserializer, Serialize};

/// NGL timestamps are kept as milliseconds since the Unix Epoch.
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

impl Default for Timestamp {
    fn default() -> Self {
        Self::now()
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_utc_datetime().format("%Y-%m-%dT%H:%M:%S%.3f%z").fmt(f)
    }
}

impl Timestamp {
    pub fn as_local_datetime(&self) -> DateTime<Local> {
        match Local.timestamp_millis_opt(self.millis) {
            LocalResult::Single(dt) => dt,
            _ => Local::now(),
        }
    }

    pub fn as_utc_datetime(&self) -> DateTime<Utc> {
        match Utc.timestamp_millis_opt(self.millis) {
            LocalResult::Single(dt) => dt,
            _ => Utc::now(),
        }
    }

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
        self.as_local_datetime().format("%Y-%m-%dT%H:%M:%S:%3f%z").to_string()
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
        self.as_local_datetime().format("%Y-%m-%dT%H:%M:%S.%3f%z").to_string()
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

    /// When you want it as an ISO-8601 date
    pub fn format_iso_8601(&self, timezone: bool) -> String {
        let ts = self.as_utc_datetime();
        if timezone {
            ts.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string()
        } else {
            ts.format("%Y-%m-%dT%H:%M:%S%.3f").to_string()
        }
    }

    /// When you want it as an RFC-3339 date
    /// (Without timezone, we use space as separator)
    pub fn format_rfc_3339(&self, timezone: bool) -> String {
        let ts = self.as_utc_datetime();
        if timezone {
            ts.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
        } else {
            ts.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
        }
    }
}

impl std::str::FromStr for Timestamp {
    type Err = chrono::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(val) = s.parse::<i64>() {
            // we always work in milliseconds
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

#[cfg(test)]
mod tests {
    use super::Timestamp;

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
