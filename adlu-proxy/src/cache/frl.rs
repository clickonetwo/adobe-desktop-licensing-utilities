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
use adlu_parse::protocol::{
    FrlActivationRequest as ActReq, FrlActivationRequestBody,
    FrlActivationResponse as ActResp, FrlActivationResponseBody, FrlAppDetails,
    FrlDeactivationQueryParams, FrlDeactivationRequest as DeactReq,
    FrlDeactivationResponse as DeactResp, FrlDeactivationResponseBody, FrlDeviceDetails,
    Request,
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
                    result.push(Request::Activation(Box::new(actr.clone())));
                    act = acts.next();
                } else {
                    result.push(Request::Deactivation(Box::new(deactr.clone())));
                    deact = deacts.next();
                }
            } else {
                result.push(Request::Activation(Box::new(actr.clone())));
                act = acts.next();
            }
        } else if let Some(deactr) = deact {
            result.push(Request::Deactivation(Box::new(deactr.clone())));
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

pub async fn store_activation_request(pool: &SqlitePool, req: &ActReq) -> Result<()> {
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
    let a_key = req.activation_id();
    debug!("Storing activation request {} with key: {}", &req.request_id, &a_key);
    let mut tx = pool.begin().await?;
    let result = sqlx::query(&i_str)
        .bind(&a_key)
        .bind(&req.deactivation_id())
        .bind(&req.api_key)
        .bind(&req.request_id)
        .bind(&req.session_id)
        .bind(&req.parsed_body.device_details.current_date)
        .bind(&req.parsed_body.npd_id)
        .bind(&req.parsed_body.asnp_template_id)
        .bind(&req.parsed_body.device_details.device_id)
        .bind(&req.parsed_body.device_details.os_user_id)
        .bind(req.parsed_body.device_details.enable_vdi_marker_exists)
        .bind(req.parsed_body.device_details.is_os_user_account_in_domain)
        .bind(req.parsed_body.device_details.is_virtual_environment)
        .bind(&req.parsed_body.device_details.os_name)
        .bind(&req.parsed_body.device_details.os_version)
        .bind(&req.parsed_body.app_details.ngl_app_id)
        .bind(&req.parsed_body.app_details.ngl_app_version)
        .bind(&req.parsed_body.app_details.ngl_lib_version)
        .bind(req.timestamp.to_db())
        .execute(&mut tx)
        .await?;
    tx.commit().await?;
    debug!("Stored activation request has rowid {}", result.last_insert_rowid());
    Ok(())
}

pub async fn store_deactivation_request(pool: &SqlitePool, req: &DeactReq) -> Result<()> {
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
    let d_key = req.deactivation_id();
    debug!("Storing deactivation request {} with key: {}", &req.request_id, &d_key);
    let mut tx = pool.begin().await?;
    let result = sqlx::query(&i_str)
        .bind(&d_key)
        .bind(&req.api_key)
        .bind(&req.request_id)
        .bind(&req.params.npd_id)
        .bind(&req.params.device_id)
        .bind(&req.params.os_user_id)
        .bind(req.params.enable_vdi_marker_exists)
        .bind(req.params.is_os_user_account_in_domain)
        .bind(req.params.is_virtual_environment)
        .bind(req.timestamp.to_db())
        .execute(&mut tx)
        .await?;
    tx.commit().await?;
    debug!("Stored deactivation request has rowid {}", result.last_insert_rowid());
    Ok(())
}

pub async fn store_activation_response(
    pool: &SqlitePool,
    req: &ActReq,
    resp: &ActResp,
) -> Result<()> {
    let field_list = "(activation_key, deactivation_key, body, timestamp)";
    let value_list = "(?, ?, ?, ?)";
    let i_str = format!(
        "insert or replace into activation_responses {} values {}",
        field_list, value_list
    );
    let a_key = req.activation_id();
    let d_key = req.deactivation_id();
    let mut tx = pool.begin().await?;
    debug!("Storing activation response {} with key: {}", &req.request_id, &a_key);
    #[cfg(feature = "parse_responses")]
    let body = serde_json::to_string(&resp.body)?;
    #[cfg(not(feature = "parse_responses"))]
    let body = resp.body.clone();
    let result = sqlx::query(&i_str)
        .bind(&a_key)
        .bind(&d_key)
        .bind(&body)
        .bind(req.timestamp.to_db())
        .execute(&mut tx)
        .await?;
    debug!("Stored activation response has rowid {}", result.last_insert_rowid());
    // remove all matching deactivation requests/responses as they are now invalid
    let d_key = req.deactivation_id();
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
    req: &DeactReq,
    resp: &DeactResp,
) -> Result<()> {
    debug!("Caching successful deactivation with request ID {}", req.request_id);
    let mut tx = pool.begin().await?;
    // first remove all earlier matching requests/responses as they are now invalid
    let d_key = req.deactivation_id();
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
    let d_key = req.deactivation_id();
    debug!("Storing deactivation response {} with key: {}", &req.request_id, &d_key);
    let result = sqlx::query(&i_str)
        .bind(&d_key)
        .bind(&resp.body)
        .bind(req.timestamp.to_db())
        .execute(&mut tx)
        .await?;
    tx.commit().await?;
    debug!("Stored deactivation response has rowid {}", result.last_insert_rowid());
    Ok(())
}

pub async fn fetch_activation_response(
    pool: &SqlitePool,
    req: &ActReq,
) -> Result<Option<ActResp>> {
    let a_key = req.activation_id();
    let q_str =
        "select body, timestamp from activation_responses where activation_key = ?";
    debug!("Finding activation response with key: {}", &a_key);
    let result = sqlx::query(q_str).bind(&a_key).fetch_optional(pool).await?;
    match result {
        Some(row) => {
            let body: String = row.get("body");
            let parsed_body: Option<FrlActivationResponseBody> =
                if cfg!(feature = "parse_responses") {
                    Some(serde_json::from_str(&body)?)
                } else {
                    None
                };
            Ok(Some(ActResp {
                request_id: req.request_id.clone(),
                timestamp: Timestamp::from_db(row.get("timestamp")),
                body,
                parsed_body,
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
    req: &DeactReq,
) -> Result<Option<DeactResp>> {
    let d_key = req.deactivation_id();
    let q_str =
        "select body, timestamp from deactivation_responses where deactivation_key = ?";
    debug!("Finding deactivation response with key: {}", &d_key);
    let result = sqlx::query(q_str).bind(&d_key).fetch_optional(pool).await?;
    match result {
        Some(row) => {
            let body: String = row.get("body");
            let parsed_body: Option<FrlDeactivationResponseBody> =
                if cfg!(feature = "parse_responses") {
                    Some(serde_json::from_str(&body)?)
                } else {
                    None
                };
            Ok(Some(DeactResp {
                request_id: req.request_id.clone(),
                timestamp: Timestamp::from_db(row.get("timestamp")),
                body,
                parsed_body,
            }))
        }
        None => {
            debug!("No deactivation response found for key: {}", &d_key);
            Ok(None)
        }
    }
}

async fn fetch_unanswered_activations(pool: &SqlitePool) -> Result<Vec<ActReq>> {
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

async fn fetch_unanswered_deactivations(pool: &SqlitePool) -> Result<Vec<DeactReq>> {
    let mut result = Vec::new();
    let q_str = r#"select * from deactivation_requests"#;
    let rows = sqlx::query(q_str).fetch_all(pool).await?;
    for row in rows.iter() {
        result.push(request_from_deactivation_row(row))
    }
    Ok(result)
}

async fn fetch_answered_activations(pool: &SqlitePool) -> Result<Vec<(ActReq, ActResp)>> {
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
) -> Result<Vec<(DeactReq, DeactResp)>> {
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

fn request_from_activation_row(row: &SqliteRow) -> ActReq {
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
    ActReq {
        api_key: row.get("api_key"),
        request_id: row.get("request_id"),
        session_id: row.get("session_id"),
        timestamp: Timestamp::from_db(row.get("timestamp")),
        parsed_body,
    }
}

fn request_from_deactivation_row(row: &SqliteRow) -> DeactReq {
    DeactReq {
        timestamp: Timestamp::from_db(row.get("timestamp")),
        api_key: row.get("api_key"),
        request_id: row.get("request_id"),
        params: FrlDeactivationQueryParams {
            npd_id: row.get("package_id"),
            device_id: row.get("device_id"),
            enable_vdi_marker_exists: row.get("is_vdi"),
            is_virtual_environment: row.get("is_virtual"),
            os_user_id: row.get("os_user_id"),
            is_os_user_account_in_domain: row.get("is_domain_user"),
        },
    }
}

fn response_from_activation_row(row: &SqliteRow) -> Result<ActResp> {
    let body: String = row.get("body");
    let parsed_body: Option<FrlActivationResponseBody> =
        if cfg!(feature = "parse_responses") {
            Some(serde_json::from_str(&body)?)
        } else {
            None
        };
    Ok(ActResp {
        request_id: row.get("request_id"),
        timestamp: Timestamp::from_db(row.get("timestamp")),
        body,
        parsed_body,
    })
}

fn response_from_deactivation_row(row: &SqliteRow) -> Result<DeactResp> {
    let body: String = row.get("body");
    let parsed_body: Option<FrlDeactivationResponseBody> =
        if cfg!(feature = "parse_responses") {
            Some(serde_json::from_str(&body)?)
        } else {
            None
        };
    Ok(DeactResp {
        request_id: row.get("request_id"),
        timestamp: Timestamp::from_db(row.get("timestamp")),
        body,
        parsed_body,
    })
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
