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
use log::debug;
use sqlx::{
    sqlite::{SqlitePool, SqliteRow},
    Row,
};

use adlu_base::Timestamp;
use adlu_parse::protocol::LogSession;

use crate::proxy::{Request, RequestType, Response};

pub async fn db_init(pool: &SqlitePool) -> Result<()> {
    sqlx::query(SESSION_SCHEMA).execute(pool).await?;
    let q_str = r#"select * from schema_version where data_type = 'log'"#;
    let u_str = "update schema_version set schema_version = ? where data_type = 'log'";
    let row = sqlx::query(q_str).fetch_one(pool).await?;
    let mut version: i64 = row.get("schema_version");
    while (version as usize) < SESSION_SCHEMA_VERSION {
        sqlx::query(SCHEMA_ALTERATIONS_BY_VERSION[(version as usize)])
            .execute(pool)
            .await?;
        version += 1;
        sqlx::query(u_str).bind(version).execute(pool).await?;
    }
    Ok(())
}

pub async fn clear(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(CLEAR_ALL).execute(&mut tx).await?;
    tx.commit().await?;
    eprintln!("Log cache has been cleared.");
    Ok(())
}

pub async fn report(
    pool: &SqlitePool,
    path: &str,
    empty: bool,
    timezone: bool,
    rfc3339: bool,
) -> Result<()> {
    let mut writer = csv::WriterBuilder::new().from_path(path)?;
    writer.write_record(report_headers(timezone))?;
    let sessions = fetch_log_sessions(pool, !empty).await?;
    for session in sessions.iter() {
        let record = report_record(session, timezone, rfc3339);
        writer.write_record(record)?;
    }
    Ok(())
}

fn report_headers(timezone: bool) -> Vec<String> {
    let time_suffix = if timezone { "" } else { " (UTC)" };
    let mut result = vec![];
    result.push("Source Address".to_string());
    result.push("Session ID".to_string());
    result.push(format!("Initial Entry{time_suffix}"));
    result.push(format!("Final Entry{time_suffix}"));
    result.push(format!("Session Start{time_suffix}"));
    result.push(format!("Session End{time_suffix}"));
    result.push("App ID".to_string());
    result.push("App Version".to_string());
    result.push("App Locale".to_string());
    result.push("NGL Version".to_string());
    result.push("OS Name".to_string());
    result.push("OS Version".to_string());
    result.push("User ID".to_string());
    result
}

fn report_record(session: &LogSession, timezone: bool, rfc3339: bool) -> Vec<String> {
    let empty = "".to_string();
    let format_ts = |ts: &Timestamp| -> String {
        if rfc3339 {
            ts.format_rfc_3339(timezone)
        } else {
            ts.format_iso_8601(timezone)
        }
    };
    let format_ots = |ots: &Option<Timestamp>| -> String {
        if let Some(ts) = ots {
            format_ts(ts)
        } else {
            empty.clone()
        }
    };
    let result = vec![
        session.source_addr.clone(),
        session.session_id.clone(),
        format_ts(&session.initial_entry),
        format_ts(&session.final_entry),
        format_ots(&session.session_start),
        format_ots(&session.session_end),
        session.app_id.as_ref().unwrap_or(&empty).clone(),
        session.app_version.as_ref().unwrap_or(&empty).clone(),
        session.app_locale.as_ref().unwrap_or(&empty).clone(),
        session.ngl_version.as_ref().unwrap_or(&empty).clone(),
        session.os_name.as_ref().unwrap_or(&empty).clone(),
        session.os_version.as_ref().unwrap_or(&empty).clone(),
        session.user_id.as_ref().unwrap_or(&empty).clone(),
    ];
    result
}

pub async fn store_upload_request(pool: &SqlitePool, req: &Request) -> Result<()> {
    let body = req.body.clone().ok_or_else(|| eyre!("{} has no body", req))?;
    let sessions =
        adlu_parse::protocol::parse_log_data(&req.source_ip, bytes::Bytes::from(body));
    for new in sessions.iter() {
        if let Some(existing) = fetch_log_session(pool, &new.session_id).await? {
            store_log_session(pool, &existing.merge(new)?).await?;
        } else {
            store_log_session(pool, new).await?;
        }
    }
    Ok(())
}

pub async fn store_upload_response(
    _pool: &SqlitePool,
    _req: &Request,
    _resp: &Response,
) -> Result<()> {
    // a no-op, since all responses are the same
    Ok(())
}

pub async fn fetch_upload_response(
    _pool: &SqlitePool,
    _req: &Request,
) -> Result<Option<Response>> {
    Ok(Some(Response {
        timestamp: Timestamp::now(),
        request_type: RequestType::LogUpload,
        status: http::StatusCode::OK,
        body: None,
        content_type: None,
        server: Some(crate::proxy::proxy_id()),
        via: None,
        request_id: None,
        session_id: None,
    }))
}

async fn fetch_log_session(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Option<LogSession>> {
    debug!("Finding log session with id: {}", session_id);
    let q_str = "select * from log_sessions where session_id = ?";
    let result = sqlx::query(q_str).bind(session_id).fetch_optional(pool).await?;
    match result {
        Some(row) => {
            debug!("Found log session with id: {}", session_id);
            Ok(Some(session_from_row(&row)))
        }
        None => {
            debug!("No log session found with id: {}", session_id);
            Ok(None)
        }
    }
}

pub(crate) async fn fetch_log_sessions(
    pool: &SqlitePool,
    info_only: bool,
) -> Result<Vec<LogSession>> {
    debug!("Fetching all log sessions");
    let mut result = vec![];
    let q_str = "select * from log_sessions";
    let rows = sqlx::query(q_str).fetch_all(pool).await?;
    for row in rows {
        let session = session_from_row(&row);
        if !info_only || session.has_info() {
            result.push(session);
        }
    }
    debug!("Fetched {} sessions", result.len());
    Ok(result)
}

async fn store_log_session(pool: &SqlitePool, session: &LogSession) -> Result<()> {
    fn opt_val(s: &Option<String>) -> String {
        match s {
            Some(s) => s.clone(),
            None => String::new(),
        }
    }
    let field_list = r#"
        (
            session_id, initial_entry, final_entry, session_start, session_end,
            app_id, app_version, app_locale, ngl_version, os_name, os_version, user_id
        )"#;
    let value_list = "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
    let i_str = format!(
        "insert or replace into log_sessions {} values {}",
        field_list, value_list
    );
    debug!("Storing log session with id: {}", &session.session_id);
    let mut tx = pool.begin().await?;
    let result = sqlx::query(&i_str)
        .bind(&session.session_id)
        .bind(session.initial_entry.to_db())
        .bind(session.final_entry.to_db())
        .bind(Timestamp::optional_to_db(&session.session_start))
        .bind(Timestamp::optional_to_db(&session.session_end))
        .bind(opt_val(&session.app_id))
        .bind(opt_val(&session.app_version))
        .bind(opt_val(&session.app_locale))
        .bind(opt_val(&session.ngl_version))
        .bind(opt_val(&session.os_name))
        .bind(opt_val(&session.os_version))
        .bind(opt_val(&session.user_id))
        .execute(&mut tx)
        .await?;
    tx.commit().await?;
    debug!("Stored log upload request has rowid {}", result.last_insert_rowid());
    Ok(())
}

fn session_from_row(row: &SqliteRow) -> LogSession {
    fn opt_val(s: String) -> Option<String> {
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
    LogSession {
        source_addr: row.get("source_addr"),
        session_id: row.get("session_id"),
        initial_entry: Timestamp::from_db(row.get("initial_entry")),
        final_entry: Timestamp::from_db(row.get("final_entry")),
        session_start: Timestamp::optional_from_db(row.get("session_start")),
        session_end: Timestamp::optional_from_db(row.get("session_end")),
        app_id: opt_val(row.get("app_id")),
        app_version: opt_val(row.get("app_version")),
        app_locale: opt_val(row.get("app_locale")),
        ngl_version: opt_val(row.get("ngl_version")),
        os_name: opt_val(row.get("os_name")),
        os_version: opt_val(row.get("os_version")),
        user_id: opt_val(row.get("user_id")),
    }
}

const SESSION_SCHEMA: &str = r#"
    create table if not exists log_sessions (
        session_id text not null unique,
        initial_entry text not null,
        final_entry text not null,
        session_start text not null,
        session_end text not null,
        app_id text not null,
        app_version text not null,
        app_locale text not null,
        ngl_version text not null,
        os_name text not null,
        os_version text not null,
        user_id text not null
    );"#;

const CLEAR_ALL: &str = r#"
    delete from log_sessions;
    "#;

const SESSION_SCHEMA_VERSION: usize = 1;

const SCHEMA_ALTERATIONS_BY_VERSION: [&str; SESSION_SCHEMA_VERSION] =
    ["alter table log_sessions add column source_addr not null default 'unknown'"];
