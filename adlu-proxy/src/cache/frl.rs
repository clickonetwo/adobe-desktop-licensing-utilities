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
use eyre::{eyre, Result, WrapErr};
use log::debug;
use sqlx::{
    sqlite::{SqlitePool, SqliteRow},
    Row,
};

use crate::proxy::{Request, RequestType, Response};
use adlu_base::Timestamp;
use adlu_parse::protocol::{
    FrlActivationRequestBody, FrlAppDetails, FrlDeactivationQueryParams, FrlDeviceDetails,
};

pub async fn clear(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(CLEAR_ALL).execute(&mut tx).await?;
    tx.commit().await?;
    eprintln!("FRL cache has been cleared.");
    Ok(())
}

pub async fn import(pool: &SqlitePool, path: &str) -> Result<()> {
    std::fs::metadata(path)?;
    // first read the forwarded pairs
    let in_pool = super::db_init(path, "rw").await?;
    db_init(&in_pool).await?;
    let activations = fetch_answered_activations(&in_pool).await?;
    let deactivations = fetch_answered_deactivations(&in_pool).await?;
    let total = activations.len() + deactivations.len();
    in_pool.close().await;
    eprintln!("Found {} forwarded request/response pair(s) to import", total);
    // now add them to the cache:
    // the activations and deactivations are each sorted in timestamp order.
    // we need to do a merge of the two in timestamp order, because activations
    // and deactivations interact with each other.  The assumption here is that
    // there are no answered requests in the local database that might conflict
    // with what we are getting from the imported set.  If that turns out to not
    // be true for some situations, more code will be needed here to check for that.
    let mut acts = activations.iter();
    let mut deacts = deactivations.iter();
    let mut act = acts.next();
    let mut deact = deacts.next();
    loop {
        if let Some(actp) = act {
            if let Some(deactp) = deact {
                if actp.0.timestamp <= deactp.0.timestamp {
                    store_activation_request(pool, &actp.0).await?;
                    store_activation_response(pool, &actp.0, &actp.1).await?;
                    act = acts.next();
                } else {
                    store_deactivation_request(pool, &deactp.0).await?;
                    store_deactivation_response(pool, &deactp.0, &deactp.1).await?;
                    deact = deacts.next();
                }
            } else {
                store_activation_request(pool, &actp.0).await?;
                store_activation_response(pool, &actp.0, &actp.1).await?;
                act = acts.next();
            }
        } else if let Some(deactp) = deact {
            store_deactivation_request(pool, &deactp.0).await?;
            store_deactivation_response(pool, &deactp.0, &deactp.1).await?;
            deact = deacts.next();
        } else {
            break;
        }
    }
    eprintln!("Completed import of request/response pairs from {path}");
    Ok(())
}

pub async fn export(pool: &SqlitePool, path: &str) -> Result<()> {
    if std::fs::metadata(path).is_ok() {
        return Err(eyre!("Cannot export to an existing file: {}", path));
    }
    // first read the unanswered requests
    let in_pool = pool;
    let activations = fetch_unanswered_activations(in_pool).await?;
    let deactivations = fetch_unanswered_deactivations(in_pool).await?;
    let total = activations.len() + deactivations.len();
    eprintln!("Found {} unanswered request(s) to export", total);
    // now store them to the export database
    let out_pool = super::db_init(path, "rwc").await?;
    db_init(&out_pool).await?;
    for act in activations.iter() {
        store_activation_request(&out_pool, act).await?;
    }
    for deact in deactivations.iter() {
        store_deactivation_request(&out_pool, deact).await?;
    }
    out_pool.close().await;
    eprintln!("Completed export of request(s) to {path}");
    Ok(())
}

pub async fn fetch_unanswered_requests(pool: &SqlitePool) -> Result<Vec<Request>> {
    let mut result = vec![];
    let activations = fetch_unanswered_activations(pool).await?;
    let deactivations = fetch_unanswered_deactivations(pool).await?;
    // now interleave them in timestamp order to make a list of requests
    let mut acts = activations.iter();
    let mut deacts = deactivations.iter();
    let mut act = acts.next();
    let mut deact = deacts.next();
    loop {
        if let Some(actr) = act {
            if let Some(deactr) = deact {
                if actr.timestamp <= deactr.timestamp {
                    result.push(actr.clone());
                    act = acts.next();
                } else {
                    result.push(deactr.clone());
                    deact = deacts.next();
                }
            } else {
                result.push(actr.clone());
                act = acts.next();
            }
        } else if let Some(deactr) = deact {
            result.push(deactr.clone());
            deact = deacts.next();
        } else {
            break;
        }
    }
    Ok(result)
}

pub async fn db_init(pool: &SqlitePool) -> Result<()> {
    sqlx::query(ACTIVATION_REQUEST_SCHEMA).execute(pool).await?;
    sqlx::query(DEACTIVATION_REQUEST_SCHEMA).execute(pool).await?;
    sqlx::query(ACTIVATION_RESPONSE_SCHEMA).execute(pool).await?;
    sqlx::query(DEACTIVATION_RESPONSE_SCHEMA).execute(pool).await?;
    Ok(())
}

pub async fn store_activation_request(pool: &SqlitePool, req: &Request) -> Result<()> {
    let body = req.body.as_ref().ok_or_else(|| eyre!("{} has no body", req))?;
    let parse = FrlActivationRequestBody::from_body(body).wrap_err(req.to_string())?;
    let field_list = r#"
        (
            activation_key, deactivation_key, api_key, request_id, session_id, device_date,
            package_id, asnp_id, device_id, os_user_id, is_vdi, is_domain_user, is_virtual,
            os_name, os_version, app_id, app_version, ngl_version, timestamp
        )"#;
    let value_list = "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
    let i_str = format!(
        "insert or replace into activation_requests {} values {}",
        field_list, value_list
    );
    let a_key = parse.activation_id();
    debug!("Storing {} with key: {}", req, &a_key);
    let mut tx = pool.begin().await?;
    let result = sqlx::query(&i_str)
        .bind(&a_key)
        .bind(&parse.deactivation_id())
        .bind(req.api_key.as_ref().ok_or_else(|| eyre!("{} has no api key", req))?)
        .bind(req.request_id.as_ref().ok_or_else(|| eyre!("{} has no request id", req))?)
        .bind(req.session_id.as_ref().ok_or_else(|| eyre!("{} has no session id", req))?)
        .bind(&parse.device_details.current_date)
        .bind(&parse.npd_id)
        .bind(&parse.asnp_template_id)
        .bind(&parse.device_details.device_id)
        .bind(&parse.device_details.os_user_id)
        .bind(parse.device_details.enable_vdi_marker_exists)
        .bind(parse.device_details.is_os_user_account_in_domain)
        .bind(parse.device_details.is_virtual_environment)
        .bind(&parse.device_details.os_name)
        .bind(&parse.device_details.os_version)
        .bind(&parse.app_details.ngl_app_id)
        .bind(&parse.app_details.ngl_app_version)
        .bind(&parse.app_details.ngl_lib_version)
        .bind(req.timestamp.to_db())
        .execute(&mut tx)
        .await?;
    tx.commit().await?;
    debug!("Stored activation request has rowid {}", result.last_insert_rowid());
    Ok(())
}

pub async fn store_deactivation_request(pool: &SqlitePool, req: &Request) -> Result<()> {
    let query = req.query.as_ref().ok_or_else(|| eyre!("{} has no query", req))?;
    let parse =
        FrlDeactivationQueryParams::from_query(query).wrap_err(req.to_string())?;
    let field_list = r#"
            (
                deactivation_key, api_key, request_id, package_id,
                device_id, os_user_id, is_vdi, is_domain_user, is_virtual,
                timestamp
            )"#;
    let value_list = "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
    let i_str = format!(
        "insert or replace into deactivation_requests {} values {}",
        field_list, value_list
    );
    let d_key = parse.deactivation_id();
    debug!("Storing {} with key: {}", req, &d_key);
    let mut tx = pool.begin().await?;
    let result = sqlx::query(&i_str)
        .bind(&d_key)
        .bind(req.api_key.as_ref().ok_or_else(|| eyre!("{} has no api key", req))?)
        .bind(&req.request_id.as_ref().ok_or_else(|| eyre!("{} has no request id", req))?)
        .bind(&parse.npd_id)
        .bind(&parse.device_id)
        .bind(&parse.os_user_id)
        .bind(parse.enable_vdi_marker_exists)
        .bind(parse.is_os_user_account_in_domain)
        .bind(parse.is_virtual_environment)
        .bind(req.timestamp.to_db())
        .execute(&mut tx)
        .await?;
    tx.commit().await?;
    debug!("Stored {} has rowid {}", req, result.last_insert_rowid());
    Ok(())
}

pub async fn store_activation_response(
    pool: &SqlitePool,
    req: &Request,
    resp: &Response,
) -> Result<()> {
    let body = req.body.as_ref().ok_or_else(|| eyre!("{} has no body", req))?;
    let parse = FrlActivationRequestBody::from_body(body).wrap_err(req.to_string())?;
    let field_list = "(activation_key, deactivation_key, body, timestamp)";
    let value_list = "(?, ?, ?, ?)";
    let i_str = format!(
        "insert or replace into activation_responses {} values {}",
        field_list, value_list
    );
    let a_key = parse.activation_id();
    let d_key = parse.deactivation_id();
    let mut tx = pool.begin().await?;
    debug!("Storing response for {} with key: {}", &req, &a_key);
    let result = sqlx::query(&i_str)
        .bind(&a_key)
        .bind(&d_key)
        .bind(
            resp.body
                .as_ref()
                .ok_or_else(|| eyre!("Response for {} has no body", req))?,
        )
        .bind(req.timestamp.to_db())
        .execute(&mut tx)
        .await?;
    debug!("Stored activation response has rowid {}", result.last_insert_rowid());
    // remove all matching deactivation requests/responses as they are now invalid
    debug!("Removing deactivation requests with key: {}", d_key);
    let d_str = "delete from deactivation_requests where deactivation_key = ?";
    sqlx::query(d_str).bind(&d_key).execute(&mut tx).await?;
    debug!("Removing deactivation responses with key: {}", d_key);
    let d_str = "delete from deactivation_responses where deactivation_key = ?";
    sqlx::query(d_str).bind(&d_key).execute(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn store_deactivation_response(
    pool: &SqlitePool,
    req: &Request,
    resp: &Response,
) -> Result<()> {
    let query = req.query.as_ref().ok_or_else(|| eyre!("{} has no query", req))?;
    let parse =
        FrlDeactivationQueryParams::from_query(query).wrap_err(req.to_string())?;
    debug!("Processing successful response to {}", req);
    let mut tx = pool.begin().await?;
    // first remove all earlier matching requests/responses as they are now invalid
    let d_key = parse.deactivation_id();
    debug!("Removing activation requests with deactivation key: {}", d_key);
    let d_str = "delete from activation_requests where deactivation_key = ?";
    sqlx::query(d_str).bind(&d_key).execute(&mut tx).await?;
    debug!("Removing activation responses with deactivation key: {}", d_key);
    let d_str = "delete from activation_responses where deactivation_key = ?";
    sqlx::query(d_str).bind(&d_key).execute(&mut tx).await?;
    // Remove any pending deactivation requests & responses as they have been completed.
    debug!("Removing deactivation requests with key: {}", d_key);
    let d_str = "delete from deactivation_requests where deactivation_key = ?";
    sqlx::query(d_str).bind(&d_key).execute(&mut tx).await?;
    debug!("Removing deactivation responses with key: {}", d_key);
    let d_str = "delete from deactivation_responses where deactivation_key = ?";
    sqlx::query(d_str).bind(&d_key).execute(&mut tx).await?;
    // then the response
    let field_list = "(deactivation_key, body, timestamp)";
    let value_list = "(?, ?, ?)";
    let i_str = format!(
        "insert or replace into deactivation_responses {} values {}",
        field_list, value_list
    );
    debug!("Storing response for {} with key: {}", req, &d_key);
    let result = sqlx::query(&i_str)
        .bind(&d_key)
        .bind(&resp.body)
        .bind(req.timestamp.to_db())
        .execute(&mut tx)
        .await?;
    tx.commit().await?;
    debug!("Stored response to {} has rowid {}", req, result.last_insert_rowid());
    Ok(())
}

pub async fn fetch_activation_response(
    pool: &SqlitePool,
    req: &Request,
) -> Result<Option<Response>> {
    let body = req.body.as_ref().ok_or_else(|| eyre!("{} has no body", req))?;
    let parse = FrlActivationRequestBody::from_body(body).wrap_err(req.to_string())?;
    let a_key = parse.activation_id();
    let q_str =
        "select body, timestamp from activation_responses where activation_key = ?";
    debug!("Finding activation response with key: {}", &a_key);
    let result = sqlx::query(q_str).bind(&a_key).fetch_optional(pool).await?;
    match result {
        Some(row) => {
            let body: String = row.get("body");
            Ok(Some(Response {
                timestamp: Timestamp::from_db(row.get("timestamp")),
                request_type: RequestType::FrlActivation,
                status: http::StatusCode::OK,
                body: Some(body),
                content_type: Some("application/json".to_string()),
                server: Some(crate::proxy::proxy_id()),
                via: None,
                request_id: req.request_id.clone(),
                session_id: None,
            }))
        }
        None => {
            debug!("No activation response found for key: {}", &a_key);
            Ok(None)
        }
    }
}

pub async fn fetch_deactivation_response(
    pool: &SqlitePool,
    req: &Request,
) -> Result<Option<Response>> {
    let query = req.query.as_ref().ok_or_else(|| eyre!("{} has no query", req))?;
    let parse =
        FrlDeactivationQueryParams::from_query(query).wrap_err(req.to_string())?;
    let d_key = parse.deactivation_id();
    let q_str =
        "select body, timestamp from deactivation_responses where deactivation_key = ?";
    debug!("Finding deactivation response with key: {}", &d_key);
    let result = sqlx::query(q_str).bind(&d_key).fetch_optional(pool).await?;
    match result {
        Some(row) => Ok(Some(response_from_parts(
            RequestType::FrlDeactivation,
            Timestamp::from_db(row.get("timestamp")),
            req.request_id.clone().ok_or_else(|| eyre!("{} has no request id", req))?,
            row.get("body"),
        ))),
        None => {
            debug!("No deactivation response found for key: {}", &d_key);
            Ok(None)
        }
    }
}

async fn fetch_unanswered_activations(pool: &SqlitePool) -> Result<Vec<Request>> {
    let mut result = Vec::new();
    let q_str = r#"select * from activation_requests req where not exists
                    (select 1 from activation_responses where
                        activation_key = req.activation_key and
                        timestamp >= req.timestamp.to_storage()
                    )"#;
    let rows = sqlx::query(q_str).fetch_all(pool).await?;
    for row in rows.iter() {
        result.push(request_from_activation_row(row))
    }
    Ok(result)
}

async fn fetch_unanswered_deactivations(pool: &SqlitePool) -> Result<Vec<Request>> {
    let mut result = Vec::new();
    let q_str = r#"select * from deactivation_requests"#;
    let rows = sqlx::query(q_str).fetch_all(pool).await?;
    for row in rows.iter() {
        result.push(request_from_deactivation_row(row))
    }
    Ok(result)
}

async fn fetch_answered_activations(
    pool: &SqlitePool,
) -> Result<Vec<(Request, Response)>> {
    let mut result = Vec::new();
    let q_str = r#"
        select req.*, resp.body from activation_requests req 
            inner join activation_responses resp
            on req.activation_key = resp.activation_key"#;
    let rows = sqlx::query(q_str).fetch_all(pool).await?;
    for row in rows.iter() {
        result
            .push((request_from_activation_row(row), response_from_activation_row(row)?));
    }
    Ok(result)
}

async fn fetch_answered_deactivations(
    pool: &SqlitePool,
) -> Result<Vec<(Request, Response)>> {
    let mut result = Vec::new();
    let q_str = r#"
        select req.*, resp.body from deactivation_requests req 
            inner join deactivation_responses resp
            on req.deactivation_key = resp.deactivation_key"#;
    let rows = sqlx::query(q_str).fetch_all(pool).await?;
    for row in rows.iter() {
        result.push((
            request_from_deactivation_row(row),
            response_from_deactivation_row(row)?,
        ));
    }
    Ok(result)
}

fn request_from_activation_row(row: &SqliteRow) -> Request {
    let device_details = FrlDeviceDetails {
        current_date: row.get("device_date"),
        device_id: row.get("device_id"),
        enable_vdi_marker_exists: row.get("is_vdi"),
        is_os_user_account_in_domain: row.get("is_domain_user"),
        is_virtual_environment: row.get("is_virtual"),
        os_name: row.get("os_name"),
        os_user_id: row.get("os_user_id"),
        os_version: row.get("os_version"),
    };
    let app_details = FrlAppDetails {
        current_asnp_id: "".to_string(),
        ngl_app_id: row.get("app_id"),
        ngl_app_version: row.get("app_version"),
        ngl_lib_version: row.get("ngl_version"),
    };
    let parsed_body = FrlActivationRequestBody {
        app_details,
        asnp_template_id: row.get("asnp_id"),
        device_details,
        npd_id: row.get("package_id"),
        npd_precedence: None,
    };
    let body = parsed_body.to_body();
    let api_key: String = row.get("api_key");
    let request_id: String = row.get("request_id");
    let session_id: String = row.get("session_id");
    Request {
        timestamp: Timestamp::from_db(row.get("timestamp")),
        request_type: RequestType::FrlActivation,
        source_ip: None,
        method: http::Method::POST,
        path: "/asnp/frl_connected/values/v2".to_string(),
        query: None,
        body: Some(body),
        content_type: Some("application/json".to_string()),
        accept_type: Some("application/json".to_string()),
        accept_language: Some("en_US".to_string()),
        user_agent: Some(crate::proxy::proxy_id()),
        via: None,
        api_key: Some(api_key),
        request_id: Some(request_id),
        session_id: Some(session_id),
        authorization: None,
    }
}

fn request_from_deactivation_row(row: &SqliteRow) -> Request {
    let params = FrlDeactivationQueryParams {
        npd_id: row.get("package_id"),
        device_id: row.get("device_id"),
        enable_vdi_marker_exists: row.get("is_vdi"),
        is_virtual_environment: row.get("is_virtual"),
        os_user_id: row.get("os_user_id"),
        is_os_user_account_in_domain: row.get("is_domain_user"),
    };
    let query = params.to_query();
    let api_key: String = row.get("api_key");
    let request_id: String = row.get("request_id");
    Request {
        timestamp: Timestamp::from_db(row.get("timestamp")),
        request_type: RequestType::FrlDeactivation,
        source_ip: None,
        method: http::Method::DELETE,
        path: "/asnp/frl_connected/v1".to_string(),
        query: Some(query),
        body: None,
        content_type: None,
        accept_type: Some("application/json".to_string()),
        accept_language: Some("en_US".to_string()),
        user_agent: Some(crate::proxy::proxy_id()),
        via: None,
        api_key: Some(api_key),
        request_id: Some(request_id),
        session_id: None,
        authorization: None,
    }
}

fn response_from_activation_row(row: &SqliteRow) -> Result<Response> {
    Ok(response_from_parts(
        RequestType::FrlActivation,
        Timestamp::from_db(row.get("timestamp")),
        row.get("request_id"),
        row.get("body"),
    ))
}

fn response_from_deactivation_row(row: &SqliteRow) -> Result<Response> {
    Ok(response_from_parts(
        RequestType::FrlDeactivation,
        Timestamp::from_db(row.get("timestamp")),
        row.get("request_id"),
        row.get("body"),
    ))
}

fn response_from_parts(
    request_type: RequestType,
    timestamp: Timestamp,
    request_id: String,
    body: String,
) -> Response {
    Response {
        timestamp,
        request_type,
        status: http::StatusCode::OK,
        body: Some(body),
        content_type: Some("application/json".to_string()),
        server: Some(crate::proxy::proxy_id()),
        via: None,
        request_id: Some(request_id),
        session_id: None,
    }
}

const ACTIVATION_REQUEST_SCHEMA: &str = r#"
    create table if not exists activation_requests (
        activation_key text not null unique,
        deactivation_key text not null,
        api_key text not null,
        request_id text not null,
        session_id text not null,
        device_date text not null,
        package_id text not null,
        asnp_id text not null,
        device_id text not null,
        os_user_id text not null,
        is_vdi boolean not null,
        is_domain_user boolean not null,
        is_virtual boolean not null,
        os_name text not null,
        os_version text not null,
        app_id text not null,
        app_version text not null,
        ngl_version text not null,
        timestamp string not null
    );
    create index if not exists deactivation_request_index on activation_requests (
        deactivation_key
    );"#;

const ACTIVATION_RESPONSE_SCHEMA: &str = r#"
    create table if not exists activation_responses (
        activation_key text not null unique,
        deactivation_key text not null,
        body text not null,
        timestamp string not null
    );
    create index if not exists deactivation_response_index on activation_responses (
        deactivation_key
    );"#;

const DEACTIVATION_REQUEST_SCHEMA: &str = r#"
    create table if not exists deactivation_requests (
        deactivation_key text not null unique,
        api_key text not null,
        request_id text not null,
        package_id text not null,
        device_id text not null,
        os_user_id text not null,
        is_domain_user boolean not null,
        is_vdi boolean not null,
        is_virtual boolean not null,
        timestamp string not null
    )"#;

const DEACTIVATION_RESPONSE_SCHEMA: &str = r#"
    create table if not exists deactivation_responses (
        deactivation_key text not null unique,
        body text not null,
        timestamp string not null
    );"#;

const CLEAR_ALL: &str = r#"
    delete from deactivation_responses;
    delete from deactivation_requests;
    delete from activation_responses;
    delete from activation_requests;
    "#;
