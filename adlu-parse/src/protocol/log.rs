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

use eyre::{eyre, Result};
use lazy_static::lazy_static;
use rand::Rng;
use regex::bytes::Regex;
use serde::{Deserialize, Serialize};
use warp::Reply;

use adlu_base::Timestamp;

#[derive(Debug, Clone)]
pub struct LogUploadRequest {
    pub timestamp: Timestamp,
    pub request_id: String,
    pub authorization: String,
    pub api_key: String,
    pub log_data: bytes::Bytes,
    pub session_data: Vec<LogSession>,
}

impl LogUploadRequest {
    pub fn to_network(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        builder
            .header("Authorization", &self.authorization)
            .header("X-Api-Key", &self.api_key)
            .body(self.log_data.clone())
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LogSession {
    pub session_id: String,
    pub initial_entry: Timestamp,
    pub final_entry: Timestamp,
    pub session_start: Option<Timestamp>,
    pub session_end: Option<Timestamp>,
    pub app_id: Option<String>,
    pub app_version: Option<String>,
    pub app_locale: Option<String>,
    pub ngl_version: Option<String>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub user_id: Option<String>,
}

impl LogSession {
    pub fn merge(&self, other: &LogSession) -> Result<Self> {
        if self.session_id != other.session_id {
            Err(eyre!("Can't merge sessions with different IDs"))
        } else {
            Ok(LogSession {
                session_id: self.session_id.clone(),
                initial_entry: if self.initial_entry <= other.initial_entry {
                    self.initial_entry.clone()
                } else {
                    other.initial_entry.clone()
                },
                final_entry: if self.final_entry >= other.final_entry {
                    self.final_entry.clone()
                } else {
                    other.final_entry.clone()
                },
                session_start: if self.session_start.is_none() {
                    other.session_start.clone()
                } else {
                    self.session_start.clone()
                },
                session_end: if self.session_end.is_none() {
                    other.session_end.clone()
                } else {
                    self.session_end.clone()
                },
                app_id: if self.app_id.is_none() {
                    other.app_id.clone()
                } else {
                    self.app_id.clone()
                },
                app_version: if self.app_version.is_none() {
                    other.app_version.clone()
                } else {
                    self.app_version.clone()
                },
                app_locale: if self.app_locale.is_none() {
                    other.app_locale.clone()
                } else {
                    self.app_locale.clone()
                },
                ngl_version: if self.ngl_version.is_none() {
                    other.ngl_version.clone()
                } else {
                    self.ngl_version.clone()
                },
                os_name: if self.os_name.is_none() {
                    other.os_name.clone()
                } else {
                    self.os_name.clone()
                },
                os_version: if self.os_version.is_none() {
                    other.os_version.clone()
                } else {
                    self.os_version.clone()
                },
                user_id: if self.user_id.is_none() {
                    other.user_id.clone()
                } else {
                    self.user_id.clone()
                },
            })
        }
    }

    pub fn mock_from_session_id(session_id: &str) -> Self {
        let start_time: Timestamp = Default::default();
        let session_len = rand::thread_rng().gen_range(2_000..=3_600_000);
        let end_time: Timestamp =
            Timestamp::from_millis(start_time.to_millis() + session_len);
        Self {
            session_id: session_id.to_string(),
            initial_entry: start_time.clone(),
            final_entry: end_time.clone(),
            session_start: Some(start_time),
            session_end: Some(end_time),
            app_id: Some("MockApp1".to_string()),
            app_version: Some("10.1.3".to_string()),
            app_locale: Some("en_US".to_string()),
            ngl_version: Some("1.26.0.5".to_string()),
            os_name: Some("MAC".to_string()),
            os_version: Some("10.12.5".to_string()),
            user_id: Some("...elided...".to_string()),
        }
    }

    pub fn to_body_string(&self) -> String {
        let session_id = &self.session_id;
        let start_time = self.initial_entry.clone();
        let end_time = self.final_entry.clone();
        let prefix = |at_start: bool| {
            if at_start {
                format!(
                    "SessionID={} Timestamp={} ...elided... Description=",
                    session_id, start_time
                )
            } else {
                format!(
                    "SessionID={} Timestamp={} ...elided... Description=",
                    session_id, end_time
                )
            }
        };
        let mut lines: Vec<String> = vec![];
        if self.session_start.is_some() {
            lines.push(format!(
                "{}\"-------- Initializing session logs --------\"",
                prefix(true)
            ));
        }
        lines.push(format!("{}\"Mock log start line\"", prefix(true)));
        if let Some(os) = &self.os_name {
            if let Some(os_version) = &self.os_version {
                lines.push(format!(
                    "{}\"SetConfig: OS Name={}, OS Version={}\"",
                    prefix(true),
                    os,
                    os_version
                ));
            }
        }
        if let Some(app) = &self.app_id {
            if let Some(app_version) = &self.app_version {
                lines.push(format!(
                    "{}\"SetConfig: AppID={}, AppVersion={}\"",
                    prefix(true),
                    app,
                    app_version
                ));
            }
        }
        if let Some(locale) = &self.app_locale {
            lines.push(format!(
                "{}\"SetAppRuntimeConfig: AppLocale={}\"",
                prefix(true),
                locale
            ));
        }
        if let Some(ngl) = &self.ngl_version {
            lines.push(format!("{}\"SetConfig: NGLLibVersion={}\"", prefix(true), ngl));
        }
        if let Some(user) = &self.user_id {
            lines.push(format!("{}\"LogCurrentUser: UserID={}\"", prefix(true), user));
        }
        lines.push(format!("{}\"Mock log end line\"", prefix(false)));
        if self.session_end.is_some() {
            lines.push(format!(
                "{}\"-------- Terminating session logs --------\"",
                prefix(false)
            ));
        }
        lines.join("\n") + "\n"
    }
}

lazy_static! {
    static ref RE_MAP: HashMap<&'static str, Regex> = {
        let mut map = HashMap::new();
        map.insert(
            "line",
            Regex::new(
                r#"(?m-u)^SessionID=([^ ]+) Timestamp=([^ ]+) .*Description="(.+)"\r?$"#,
            )
            .unwrap(),
        );
        map.insert("start", Regex::new(r"(?-u)Initializing session logs").unwrap());
        map.insert("end", Regex::new(r"(?-u)Terminating session logs").unwrap());
        map.insert(
            "os",
            Regex::new(r"(?-u)SetConfig:.+OS Name=([^,]+), OS Version=([^\s]+)").unwrap(),
        );
        map.insert(
            "app",
            Regex::new(r"(?-u)SetConfig:.+AppID=([^,]+), AppVersion=([^,]+)").unwrap(),
        );
        map.insert("ngl", Regex::new(r"(?-u)SetConfig:.+NGLLibVersion=([^,]+)").unwrap());
        map.insert(
            "locale",
            Regex::new(r"(?-u)SetAppRuntimeConfig:.+AppLocale=([a-zA-Z_]+)").unwrap(),
        );
        map.insert(
            "user",
            Regex::new(r"(?-u)LogCurrentUser:.+UserID=([a-z0-9]{40})").unwrap(),
        );
        map
    };
}

pub fn parse_log_data(data: bytes::Bytes) -> Vec<LogSession> {
    let line_pattern = &RE_MAP["line"];
    let mut sessions: Vec<LogSession> = Vec::new();
    let mut session: LogSession = Default::default();
    for cap in line_pattern.captures_iter(&data) {
        let sid = String::from_utf8(cap[1].to_vec()).unwrap();
        let time = String::from_utf8(cap[2].to_vec()).unwrap();
        let timestamp = Timestamp::from_log(&time);
        if sid != session.session_id {
            if !session.session_id.is_empty() {
                sessions.push(session.clone())
            }
            session = LogSession {
                session_id: sid,
                initial_entry: timestamp.clone(),
                final_entry: timestamp.clone(),
                ..Default::default()
            }
        }
        parse_log_description(&mut session, &timestamp, &cap[3]);
    }
    if !session.session_id.is_empty() {
        sessions.push(session.clone())
    }
    sessions
}

fn parse_log_description(
    session: &mut LogSession,
    timestamp: &Timestamp,
    description: &[u8],
) {
    session.final_entry = timestamp.clone();
    if RE_MAP["start"].captures(description).is_some() {
        session.session_start = Some(timestamp.clone());
    } else if RE_MAP["end"].captures(description).is_some() {
        session.session_end = Some(timestamp.clone());
    } else if let Some(cap) = RE_MAP["os"].captures(description) {
        let os_name = String::from_utf8(cap[1].to_vec()).unwrap();
        let os_version = String::from_utf8(cap[2].to_vec()).unwrap();
        session.os_name = Some(os_name);
        session.os_version = Some(os_version);
    } else if let Some(cap) = RE_MAP["app"].captures(description) {
        let app_id = String::from_utf8(cap[1].to_vec()).unwrap();
        let app_version = String::from_utf8(cap[2].to_vec()).unwrap();
        session.app_id = Some(app_id);
        session.app_version = Some(app_version);
    } else if let Some(cap) = RE_MAP["ngl"].captures(description) {
        let ngl_version = String::from_utf8(cap[1].to_vec()).unwrap();
        session.ngl_version = Some(ngl_version);
    } else if let Some(cap) = RE_MAP["locale"].captures(description) {
        let locale = String::from_utf8(cap[1].to_vec()).unwrap();
        session.app_locale = Some(locale);
    } else if let Some(cap) = RE_MAP["user"].captures(description) {
        let user_id = String::from_utf8(cap[1].to_vec()).unwrap();
        session.user_id = Some(user_id);
    }
}

#[derive(Debug, Clone)]
pub struct LogUploadResponse {
    pub timestamp: Timestamp,
}

impl Default for LogUploadResponse {
    fn default() -> Self {
        LogUploadResponse { timestamp: Timestamp::now() }
    }
}

impl LogUploadResponse {
    pub fn new() -> Self {
        Default::default()
    }

    pub async fn from_network(_response: reqwest::Response) -> Result<Self> {
        Ok(Self::new())
    }
}

impl From<LogUploadResponse> for warp::reply::Response {
    fn from(_resp: LogUploadResponse) -> Self {
        warp::reply().into_response()
    }
}

impl Reply for LogUploadResponse {
    fn into_response(self) -> warp::reply::Response {
        self.into()
    }
}

#[cfg(test)]
mod test {
    use std::fs::read_to_string;

    use crate::protocol::log::LogSession;
    use adlu_base::Timestamp;

    #[test]
    fn test_parse_complete_log_upload() {
        let path = "../rsrc/logs/mac/NGLClient_PremierePro122.5.0.log.bin";
        let data = bytes::Bytes::from(read_to_string(path).unwrap());
        let sessions = super::parse_log_data(data);
        assert_eq!(sessions.len(), 1);
        let session = &sessions[0];
        let session_id = "4f7c3960-48da-49bb-9359-e0f040ecae66.1660326622129";
        let start = Timestamp::from_db("2022-08-12T10:50:22:129-0700");
        let end = Timestamp::from_db("2022-08-12T10:50:53:807-0700");
        assert_eq!(session.session_id, session_id);
        assert_eq!(session.initial_entry, start);
        assert_eq!(session.session_start.as_ref().unwrap(), &session.initial_entry);
        assert_eq!(session.final_entry, end);
        assert_eq!(session.session_end.as_ref().unwrap(), &session.final_entry);
        assert_eq!(session.app_id.as_ref().unwrap(), "PremierePro1");
        assert_eq!(session.app_version.as_ref().unwrap(), "22.5.0");
        assert_eq!(session.ngl_version.as_ref().unwrap(), "1.30.0.1");
        assert_eq!(session.app_locale.as_ref().unwrap(), "en_US");
        assert_eq!(
            session.user_id.as_ref().unwrap(),
            "9f22a90139cbb9f1676b0113e1fb574976dc550a"
        );
    }

    #[test]
    fn test_parse_partial_log_upload() {
        let path = "../rsrc/logs/mac/NGLClient_AcrobatDC122.1.20169.7.log.bin";
        let data = bytes::Bytes::from(read_to_string(path).unwrap());
        let sessions = super::parse_log_data(data);
        assert_eq!(sessions.len(), 1);
        let session = &sessions[0];
        let session_id = "e6ab2d44-5909-4838-a79f-5091f5736073.1659806990834";
        let start = Timestamp::from_db("2022-08-08T09:25:33:720-0700");
        let end = Timestamp::from_db("2022-08-08T09:25:33:720-0700");
        assert_eq!(session.session_id, session_id);
        assert_eq!(session.initial_entry, start);
        assert!(session.session_start.is_none());
        assert_eq!(session.final_entry, end);
        assert_eq!(session.session_end.as_ref().unwrap(), &session.final_entry);
        assert!(session.app_id.is_none());
        assert!(session.app_version.is_none());
        assert!(session.ngl_version.is_none());
        assert!(session.app_locale.is_none());
        assert!(session.user_id.is_none());
    }

    #[test]
    fn test_parse_win_unterminated_log_upload() {
        let path = "../rsrc/logs/win/NGLClient_Illustrator126.4.1.log.bin";
        let data = bytes::Bytes::from(read_to_string(path).unwrap());
        let sessions = super::parse_log_data(data);
        assert_eq!(sessions.len(), 1);
        let session = &sessions[0];
        let session_id = "bc532766-d56c-43fe-aaba-eb5f4323a53c.1660495166236";
        let start = Timestamp::from_db("2022-08-14T09:39:26:236-0700");
        let end = Timestamp::from_db("2022-08-14T09:39:45:536-0700");
        assert_eq!(session.session_id, session_id);
        assert_eq!(session.initial_entry, start);
        assert_eq!(session.session_start.as_ref().unwrap(), &session.initial_entry);
        assert_eq!(session.final_entry, end);
        assert!(session.session_end.is_none());
        assert_eq!(session.app_id.as_ref().unwrap(), "Illustrator1");
        assert_eq!(session.app_version.as_ref().unwrap(), "26.4.1");
        assert_eq!(session.ngl_version.as_ref().unwrap(), "1.30.0.2");
        assert_eq!(session.app_locale.as_ref().unwrap(), "en_US");
        assert_eq!(
            session.user_id.as_ref().unwrap(),
            "9f22a90139cbb9f1676b0113e1fb574976dc550a"
        );
    }

    #[test]
    fn test_parse_mock_log_upload() {
        let data = bytes::Bytes::from(
            LogSession::mock_from_session_id("test-id").to_body_string(),
        );
        let sessions = super::parse_log_data(data);
        assert_eq!(sessions.len(), 1);
        let session = &sessions[0];
        assert_eq!(session.session_id, "test-id");
        assert_eq!(session.initial_entry, session.session_start.clone().unwrap());
        assert!(session.os_name.is_some());
        assert!(session.os_version.is_some());
        assert!(session.app_id.is_some());
        assert!(session.app_version.is_some());
        assert!(session.app_locale.is_some());
        assert!(session.ngl_version.is_some());
        assert_eq!(session.final_entry, session.session_end.clone().unwrap());
    }
}