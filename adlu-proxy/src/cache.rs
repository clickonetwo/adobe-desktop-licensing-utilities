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
use std::{env, str::FromStr, sync::Arc};

use dialoguer::Confirm;
use eyre::{eyre, Result, WrapErr};
use log::{debug, error, info};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteRow},
    ConnectOptions, Row,
};

use adlu_base::Timestamp;
use adlu_parse::protocol::{
    FrlActivationRequest as ActReq, FrlActivationRequestBody,
    FrlActivationResponse as ActResp, FrlActivationResponseBody, FrlAppDetails,
    FrlDeactivationQueryParams, FrlDeactivationRequest as DeactReq,
    FrlDeactivationResponse as DeactResp, FrlDeactivationResponseBody, FrlDeviceDetails,
    FrlRequest, FrlResponse,
};

use crate::settings::{ProxyMode, Settings};

/// A cache for requests and responses.
///
/// This cache uses an SQLite v3 database accessed asynchronously via `sqlx`.
///
/// The cache uses an [`Arc`] internally, so you can just allocate one globally
/// and use it everywhere.
#[derive(Debug, Clone)]
pub struct Cache {
    inner: Arc<CacheRef>,
}

#[derive(Debug, Default)]
struct CacheRef {
    enabled: bool,
    mode: ProxyMode,
    db_pool: Option<SqlitePool>,
}

impl Cache {
    pub async fn from(conf: &Settings, can_create: bool) -> Result<Self> {
        if let ProxyMode::Passthrough = conf.proxy.mode {
            return Ok(Cache { inner: Arc::new(CacheRef::default()) });
        }
        let db_name = &conf.cache.db_path;
        let mode = if can_create {
            "rwc"
        } else {
            std::fs::metadata(db_name)
                .wrap_err(format!("Can't open cache db: {}", db_name))?;
            "rw"
        };
        let pool = db_init(db_name, mode)
            .await
            .wrap_err(format!("Can't connect to cache db: {}", db_name))?;
        info!("Valid cache database: {}", &db_name);
        Ok(Cache {
            inner: Arc::new(CacheRef {
                enabled: true,
                mode: conf.proxy.mode.clone(),
                db_pool: Some(pool),
            }),
        })
    }

    pub async fn close(&self) {
        if self.inner.enabled {
            if let Some(pool) = &self.inner.db_pool {
                if !pool.is_closed() {
                    pool.close().await;
                }
            }
        }
    }

    pub async fn clear(&self, yes: bool) -> Result<()> {
        let confirm = match yes {
            true => true,
            false => Confirm::new()
                .with_prompt("Really clear the cache? This operation cannot be undone.")
                .default(false)
                .show_default(true)
                .interact()?,
        };
        if confirm {
            let pool = self.inner.db_pool.as_ref().unwrap();
            let mut tx = pool.begin().await?;
            sqlx::query(CLEAR_ALL).execute(&mut tx).await?;
            tx.commit().await?;
            eprintln!("Cache has been cleared.");
        }
        Ok(())
    }

    pub async fn import(&self, path: &str) -> Result<()> {
        std::fs::metadata(path)?;
        // first read the forwarded pairs
        let in_pool = db_init(path, "rw").await?;
        let pairs = fetch_forwarded_pairs(&in_pool).await?;
        in_pool.close().await;
        eprintln!("Found {} forwarded request/response pair(s) to import", pairs.len());
        // now add them to the cache:
        // these are already sorted in timestamp order, and we import them in that order,
        // because they the activations and deactivations interact with each other.
        for (req, resp) in pairs.iter() {
            self.store_request(req).await;
            self.store_response(req, resp).await;
        }
        eprintln!("Completed import of request/response pairs from {path}");
        Ok(())
    }

    pub async fn export(&self, path: &str) -> Result<()> {
        if std::fs::metadata(path).is_ok() {
            return Err(eyre!("Cannot export to an existing file: {}", path));
        }
        // first read the unanswered requests
        let in_pool = self.inner.db_pool.as_ref().unwrap();
        let reqs = fetch_unanswered_requests(in_pool).await?;
        eprintln!("Found {} unanswered request(s) to export", reqs.len());
        // now store them to the other database
        let out_pool = db_init(path, "rwc").await?;
        for req in reqs.iter() {
            self.store_request_to_pool(req, &out_pool).await;
        }
        out_pool.close().await;
        eprintln!("Completed export of request(s) to {path}");
        Ok(())
    }

    pub async fn store_request(&self, req: &FrlRequest) {
        if !self.inner.enabled {
            return;
        }
        let pool = self.inner.db_pool.as_ref().unwrap();
        self.store_request_to_pool(req, pool).await;
    }

    async fn store_request_to_pool(&self, req: &FrlRequest, pool: &SqlitePool) {
        let result = match req {
            FrlRequest::Activation(req) => store_activation_request(pool, req).await,
            FrlRequest::Deactivation(req) => store_deactivation_request(pool, req).await,
        };
        if let Err(err) = result {
            let id = req.request_id();
            error!("Cache store of request ID {} failed: {}", id, err);
        }
    }

    pub async fn store_response(&self, req: &FrlRequest, resp: &FrlResponse) {
        if !self.inner.enabled {
            return;
        }
        let id = req.request_id();
        let pool = self.inner.db_pool.as_ref().unwrap();
        let mode = self.inner.mode.clone();
        let result = match resp {
            FrlResponse::Activation(resp) => {
                if let FrlRequest::Activation(req) = req {
                    store_activation_response(mode, pool, req, resp).await
                } else {
                    panic!("Internal activation cache inconsistency: please report a bug")
                }
            }
            FrlResponse::Deactivation(resp) => {
                if let FrlRequest::Deactivation(req) = req {
                    store_deactivation_response(mode, pool, req, resp).await
                } else {
                    panic!(
                        "Internal deactivation cache inconsistency: please report a bug"
                    )
                }
            }
        };
        if let Err(err) = result {
            error!("Cache store of request ID {} failed: {}", id, err);
        }
    }

    pub async fn fetch_response(&self, req: &FrlRequest) -> Option<FrlResponse> {
        if !self.inner.enabled {
            return None;
        }
        let pool = self.inner.db_pool.as_ref().unwrap();
        let result = match req {
            FrlRequest::Activation(request) => {
                match fetch_activation_response(pool, request).await {
                    Ok(Some(resp)) => Ok(Some(FrlResponse::Activation(Box::new(resp)))),
                    Ok(None) => Ok(None),
                    Err(err) => Err(err),
                }
            }
            FrlRequest::Deactivation(request) => {
                match fetch_deactivation_response(pool, request).await {
                    Ok(Some(resp)) => Ok(Some(FrlResponse::Deactivation(Box::new(resp)))),
                    Ok(None) => Ok(None),
                    Err(err) => Err(err),
                }
            }
        };
        match result {
            Err(err) => {
                let id = req.request_id();
                error!("Cache fetch of request ID {} failed: {}", id, err);
                None
            }
            Ok(val) => val,
        }
    }

    pub async fn fetch_forwarding_requests(&self) -> Vec<FrlRequest> {
        if !self.inner.enabled {
            return Vec::new();
        }
        let pool = self.inner.db_pool.as_ref().unwrap();
        match fetch_unanswered_requests(pool).await {
            Ok(result) => result,
            Err(err) => {
                error!("Fetch of forwarding requests failed: {:?}", err);
                Vec::new()
            }
        }
    }
}

async fn db_init(db_name: &str, mode: &str) -> Result<SqlitePool> {
    let db_url = format!("file:{}?mode={}", db_name, mode);
    let mut options = SqliteConnectOptions::from_str(&db_url).map_err(|e| eyre!(e))?;
    if env::var("FRL_PROXY_ENABLE_STATEMENT_LOGGING").is_err() {
        options.disable_statement_logging();
    }
    let pool = SqlitePoolOptions::new().max_connections(5).connect_with(options).await?;
    sqlx::query(ACTIVATION_REQUEST_SCHEMA).execute(&pool).await?;
    sqlx::query(DEACTIVATION_REQUEST_SCHEMA).execute(&pool).await?;
    sqlx::query(ACTIVATION_RESPONSE_SCHEMA).execute(&pool).await?;
    sqlx::query(DEACTIVATION_RESPONSE_SCHEMA).execute(&pool).await?;
    Ok(pool)
}

async fn store_activation_request(pool: &SqlitePool, req: &ActReq) -> Result<()> {
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
        .bind(req.timestamp.to_storage())
        .execute(&mut tx)
        .await?;
    tx.commit().await?;
    debug!("Stored activation request has rowid {}", result.last_insert_rowid());
    Ok(())
}

async fn store_deactivation_request(pool: &SqlitePool, req: &DeactReq) -> Result<()> {
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
        .bind(req.timestamp.to_storage())
        .execute(&mut tx)
        .await?;
    tx.commit().await?;
    debug!("Stored deactivation request has rowid {}", result.last_insert_rowid());
    Ok(())
}

async fn store_activation_response(
    mode: ProxyMode,
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
        .bind(req.timestamp.to_storage())
        .execute(&mut tx)
        .await?;
    debug!("Stored activation response has rowid {}", result.last_insert_rowid());
    if let ProxyMode::Forward = mode {
        // if we are forwarding, then we just remember the response
    } else {
        // otherwise we remove all stored deactivation requests/responses as they are now invalid
        let d_key = req.deactivation_id();
        debug!("Removing deactivation requests with key: {}", d_key);
        let d_str = "delete from deactivation_requests where deactivation_key = ?";
        sqlx::query(d_str).bind(&d_key).execute(&mut tx).await?;
        debug!("Removing deactivation responses with key: {}", d_key);
        let d_str = "delete from deactivation_responses where deactivation_key = ?";
        sqlx::query(d_str).bind(&d_key).execute(&mut tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn store_deactivation_response(
    mode: ProxyMode,
    pool: &SqlitePool,
    req: &DeactReq,
    resp: &DeactResp,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    if let ProxyMode::Forward = mode {
        // when we are forwarding, we store the response for later processing
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
            .bind(req.timestamp.to_storage())
            .execute(&mut tx)
            .await?;
        debug!("Stored deactivation response has rowid {}", result.last_insert_rowid());
    } else {
        // when we are live, we remove all activation requests/responses as they are now invalid
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
    }
    tx.commit().await?;
    Ok(())
}

async fn fetch_activation_response(
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
                timestamp: Timestamp::from_storage(row.get("timestamp")),
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

async fn fetch_deactivation_response(
    pool: &SqlitePool,
    req: &DeactReq,
) -> Result<Option<DeactResp>> {
    let d_key = req.deactivation_id();
    let q_str = "select body from deactivation_responses where deactivation_key = ?";
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
                timestamp: Timestamp::from_storage(row.get("timestamp")),
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

async fn fetch_unanswered_requests(pool: &SqlitePool) -> Result<Vec<FrlRequest>> {
    let mut activations = fetch_unanswered_activations(pool).await?;
    let mut deactivations = fetch_unanswered_deactivations(pool).await?;
    activations.append(&mut deactivations);
    activations.sort_unstable_by(|r1, r2| r1.timestamp().cmp(r2.timestamp()));
    Ok(activations)
}

async fn fetch_unanswered_activations(pool: &SqlitePool) -> Result<Vec<FrlRequest>> {
    let mut result = Vec::new();
    let q_str = r#"select * from activation_requests req where not exists
                    (select 1 from activation_responses where
                        activation_key = req.activation_key and
                        timestamp >= req.timestamp.to_storage()
                    )"#;
    let rows = sqlx::query(q_str).fetch_all(pool).await?;
    for row in rows.iter() {
        result.push(FrlRequest::Activation(Box::new(request_from_activation_row(row))))
    }
    Ok(result)
}

async fn fetch_unanswered_deactivations(pool: &SqlitePool) -> Result<Vec<FrlRequest>> {
    let mut result = Vec::new();
    let q_str = r#"select * from deactivation_requests"#;
    let rows = sqlx::query(q_str).fetch_all(pool).await?;
    for row in rows.iter() {
        result
            .push(FrlRequest::Deactivation(Box::new(request_from_deactivation_row(row))))
    }
    Ok(result)
}

async fn fetch_forwarded_pairs(
    pool: &SqlitePool,
) -> Result<Vec<(FrlRequest, FrlResponse)>> {
    let mut activations = fetch_forwarded_activations(pool).await?;
    let mut deactivations = fetch_forwarded_deactivations(pool).await?;
    activations.append(&mut deactivations);
    activations.sort_unstable_by(|r1, r2| r1.0.timestamp().cmp(r2.0.timestamp()));
    Ok(activations)
}

async fn fetch_forwarded_activations(
    pool: &SqlitePool,
) -> Result<Vec<(FrlRequest, FrlResponse)>> {
    let mut result = Vec::new();
    let q_str = r#"
        select req.*, resp.body from activation_requests req 
            inner join activation_responses resp
            on req.activation_key = resp.activation_key"#;
    let rows = sqlx::query(q_str).fetch_all(pool).await?;
    for row in rows.iter() {
        result.push((
            FrlRequest::Activation(Box::new(request_from_activation_row(row))),
            FrlResponse::Activation(Box::new(response_from_activation_row(row)?)),
        ))
    }
    Ok(result)
}

async fn fetch_forwarded_deactivations(
    pool: &SqlitePool,
) -> Result<Vec<(FrlRequest, FrlResponse)>> {
    let mut result = Vec::new();
    let q_str = r#"
        select req.*, resp.body from deactivation_requests req 
            inner join deactivation_responses resp
            on req.deactivation_key = resp.deactivation_key"#;
    let rows = sqlx::query(q_str).fetch_all(pool).await?;
    for row in rows.iter() {
        result.push((
            FrlRequest::Deactivation(Box::new(request_from_deactivation_row(row))),
            FrlResponse::Deactivation(Box::new(response_from_deactivation_row(row)?)),
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
        timestamp: Timestamp::from_storage(row.get("timestamp")),
        parsed_body,
    }
}

fn request_from_deactivation_row(row: &SqliteRow) -> DeactReq {
    DeactReq {
        timestamp: Timestamp::from_storage(row.get("timestamp")),
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
        timestamp: Timestamp::from_storage(row.get("timestamp")),
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
        timestamp: Timestamp::from_storage(row.get("timestamp")),
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
