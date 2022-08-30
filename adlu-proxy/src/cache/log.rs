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
use adlu_parse::protocol::{LogSession, LogUploadRequest, LogUploadResponse};
use eyre::Result;
use log::debug;
use sqlx::{
    sqlite::{SqlitePool, SqliteRow},
    Row,
};

pub async fn db_init(pool: &SqlitePool) -> Result<()> {
    sqlx::query(SESSION_SCHEMA).execute(pool).await?;
    Ok(())
}

pub async fn clear(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(CLEAR_ALL).execute(&mut tx).await?;
    tx.commit().await?;
    eprintln!("Log cache has been cleared.");
    Ok(())
}

pub async fn report(pool: &SqlitePool, path: &str) -> Result<()> {
    let mut writer = csv::WriterBuilder::new().from_path(path)?;
    let sessions = fetch_log_sessions(pool).await?;
    for session in sessions.iter() {
        writer.serialize(session)?;
    }
    Ok(())
}

pub async fn store_upload_request(
    pool: &SqlitePool,
    req: &LogUploadRequest,
) -> Result<()> {
    let sessions = &req.session_data;
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
    _req: &LogUploadRequest,
    _resp: &LogUploadResponse,
) -> Result<()> {
    // a no-op, since all responses are the same
    Ok(())
}

pub async fn fetch_upload_response(
    _pool: &SqlitePool,
    _req: &LogUploadRequest,
) -> Result<Option<LogUploadResponse>> {
    Ok(Some(LogUploadResponse::new()))
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

pub(crate) async fn fetch_log_sessions(pool: &SqlitePool) -> Result<Vec<LogSession>> {
    debug!("Fetching all log sessions");
    let mut result = vec![];
    let q_str = "select * from log_sessions";
    let rows = sqlx::query(q_str).fetch_all(pool).await?;
    for row in rows {
        result.push(session_from_row(&row));
    }
    debug!("Fetched {} sessions", result.len());
    Ok(result)
}

async fn store_log_session(pool: &SqlitePool, session: &LogSession) -> Result<()> {
    fn optval(s: &Option<String>) -> String {
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
        .bind(optval(&session.app_id))
        .bind(optval(&session.app_version))
        .bind(optval(&session.app_locale))
        .bind(optval(&session.ngl_version))
        .bind(optval(&session.os_name))
        .bind(optval(&session.os_version))
        .bind(optval(&session.user_id))
        .execute(&mut tx)
        .await?;
    tx.commit().await?;
    debug!("Stored log upload request has rowid {}", result.last_insert_rowid());
    Ok(())
}

fn session_from_row(row: &SqliteRow) -> LogSession {
    fn optval(s: String) -> Option<String> {
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
    LogSession {
        session_id: row.get("session_id"),
        initial_entry: Timestamp::from_db(row.get("initial_entry")),
        final_entry: Timestamp::from_db(row.get("final_entry")),
        session_start: Timestamp::optional_from_db(row.get("session_start")),
        session_end: Timestamp::optional_from_db(row.get("session_end")),
        app_id: optval(row.get("app_id")),
        app_version: optval(row.get("app_version")),
        app_locale: optval(row.get("app_locale")),
        ngl_version: optval(row.get("ngl_version")),
        os_name: optval(row.get("os_name")),
        os_version: optval(row.get("os_version")),
        user_id: optval(row.get("user_id")),
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
