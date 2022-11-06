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
use adlu_base::Timestamp;
use adlu_parse::protocol::{LicenseSession, NulLicenseRequestBody};
use eyre::{eyre, Result, WrapErr};
use log::debug;
use sqlx::{
    sqlite::{SqlitePool, SqliteRow},
    Row,
};

use crate::proxy::{Request, Response};

pub async fn db_init(pool: &SqlitePool) -> Result<()> {
    sqlx::query(SESSION_SCHEMA).execute(pool).await?;
    Ok(())
}

pub async fn clear(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(CLEAR_ALL).execute(&mut tx).await?;
    tx.commit().await?;
    eprintln!("Launch cache has been cleared.");
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
    let sessions = fetch_license_sessions(pool, !empty).await?;
    for session in sessions.iter() {
        let record = report_record(session, timezone, rfc3339);
        writer.write_record(record)?;
    }
    Ok(())
}

fn report_headers(timezone: bool) -> Vec<String> {
    let time_suffix = if timezone { "" } else { " (UTC)" };
    let mut result = vec![];
    result.push("Session ID".to_string());
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

fn report_record(session: &LicenseSession, timezone: bool, rfc3339: bool) -> Vec<String> {
    let format_ts = |ts: &Timestamp| -> String {
        if rfc3339 {
            ts.format_rfc_3339(timezone)
        } else {
            ts.format_iso_8601(timezone)
        }
    };
    let result = vec![
        session.session_id.clone(),
        format_ts(&session.session_start),
        format_ts(&session.session_end),
        session.app_id.clone(),
        session.app_version.clone(),
        session.app_locale.clone(),
        session.ngl_version.clone(),
        session.os_name.clone(),
        session.os_version.clone(),
        session.user_id.clone(),
    ];
    result
}

pub async fn store_license_request(pool: &SqlitePool, req: &Request) -> Result<()> {
    let body = req.body.as_ref().ok_or_else(|| eyre!("{} has no body", req))?;
    let parse = NulLicenseRequestBody::from_body(body).wrap_err(req.to_string())?;
    let new = LicenseSession::from_parts(
        &req.timestamp,
        req.session_id.as_ref().ok_or_else(|| eyre!("{} has no session id", req))?,
        &parse,
    );
    if let Some(existing) = fetch_license_session(pool, &new.session_id).await? {
        store_license_session(pool, &existing.merge(new)?).await?;
    } else {
        store_license_session(pool, &new).await?;
    }
    Ok(())
}

pub async fn store_license_response(
    _pool: &SqlitePool,
    _req: &Request,
    _resp: &Response,
) -> Result<()> {
    // a no-op, since we don't store NUL license info
    Ok(())
}

pub async fn fetch_license_response(
    _pool: &SqlitePool,
    _req: &Request,
) -> Result<Option<Response>> {
    // a no-op, since we don't store NUL license info
    Ok(None)
}

async fn fetch_license_session(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Option<LicenseSession>> {
    debug!("Finding license session with id: {}", session_id);
    let q_str = "select * from license_sessions where session_id = ?";
    let result = sqlx::query(q_str).bind(session_id).fetch_optional(pool).await?;
    match result {
        Some(row) => {
            debug!("Found license session with id: {}", session_id);
            Ok(Some(session_from_row(&row)))
        }
        None => {
            debug!("No license session found with id: {}", session_id);
            Ok(None)
        }
    }
}

pub(crate) async fn fetch_license_sessions(
    pool: &SqlitePool,
    _info_only: bool,
) -> Result<Vec<LicenseSession>> {
    debug!("Fetching all license sessions");
    let mut result = vec![];
    let q_str = "select * from license_sessions";
    let rows = sqlx::query(q_str).fetch_all(pool).await?;
    for row in rows {
        let session = session_from_row(&row);
        // all launch sessions have info
        result.push(session);
    }
    debug!("Fetched {} sessions", result.len());
    Ok(result)
}

async fn store_license_session(
    pool: &SqlitePool,
    session: &LicenseSession,
) -> Result<()> {
    let field_list = r#"
        (
            session_id, session_start, session_end,
            app_id, app_version, app_locale, ngl_version, os_name, os_version, user_id
        )"#;
    let value_list = "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
    let i_str = format!(
        "insert or replace into license_sessions {} values {}",
        field_list, value_list
    );
    debug!("Storing license session with id: {}", &session.session_id);
    let mut tx = pool.begin().await?;
    let result = sqlx::query(&i_str)
        .bind(&session.session_id)
        .bind(Timestamp::to_db(&session.session_start))
        .bind(Timestamp::to_db(&session.session_end))
        .bind(&session.app_id)
        .bind(&session.app_version)
        .bind(&session.app_locale)
        .bind(&session.ngl_version)
        .bind(&session.os_name)
        .bind(&session.os_version)
        .bind(&session.user_id)
        .execute(&mut tx)
        .await?;
    tx.commit().await?;
    debug!("Stored license session request has rowid {}", result.last_insert_rowid());
    Ok(())
}

fn session_from_row(row: &SqliteRow) -> LicenseSession {
    LicenseSession {
        session_id: row.get("session_id"),
        session_start: Timestamp::from_db(row.get("session_start")),
        session_end: Timestamp::from_db(row.get("session_end")),
        app_id: row.get("app_id"),
        app_version: row.get("app_version"),
        app_locale: row.get("app_locale"),
        ngl_version: row.get("ngl_version"),
        os_name: row.get("os_name"),
        os_version: row.get("os_version"),
        user_id: row.get("user_id"),
    }
}

const SESSION_SCHEMA: &str = r#"
    create table if not exists license_sessions (
        session_id text not null unique,
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
    delete from license_sessions;
    "#;
