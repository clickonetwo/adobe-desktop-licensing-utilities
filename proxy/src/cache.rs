/*
Copyright 2020 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
use crate::cops::{Kind, Request as CRequest, Response as CResponse};
use crate::settings::{ProxyMode, Settings};
use dialoguer::Confirm;
use eyre::{eyre, Result, WrapErr};
use log::{debug, error, info};
use sqlx::sqlite::SqliteRow;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions},
    ConnectOptions, Row,
};
use std::{env, str::FromStr, sync::Arc};

#[derive(Default)]
pub struct Cache {
    enabled: bool,
    mode: ProxyMode,
    db_pool: Option<SqlitePool>,
}

impl Cache {
    pub async fn from(conf: &Settings, can_create: bool) -> Result<Arc<Cache>> {
        if let ProxyMode::Passthrough = conf.proxy.mode {
            return Ok(Arc::new(Cache::default()));
        }
        let db_name = &conf.cache.db_path;
        let mode = if can_create {
            "rwc"
        } else {
            std::fs::metadata(db_name)?;
            "rw"
        };
        let pool = db_init(db_name, mode)
            .await
            .wrap_err(format!("Can't connect to cache db: {}", db_name))?;
        info!("Valid cache database: {}", &db_name);
        Ok(Arc::new(Cache {
            enabled: true,
            mode: conf.proxy.mode.clone(),
            db_pool: Some(pool),
        }))
    }

    pub async fn close(&self) {
        if self.enabled {
            if let Some(pool) = &self.db_pool {
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
            let pool = self.db_pool.as_ref().unwrap();
            let mut tx = pool.begin().await?;
            sqlx::query(CLEAR_ALL).execute(&mut tx).await?;
            tx.commit().await?;
            eprintln!("Cache has been cleared.");
        }
        self.close().await;
        Ok(())
    }

    pub async fn import(&self, path: &str) -> Result<()> {
        std::fs::metadata(path)?;
        // first read the forwarded pairs
        let in_pool = db_init(path, "ro").await?;
        let pairs = fetch_forwarded_pairs(&in_pool).await?;
        in_pool.close().await;
        // now add them to the cache
        let out_pool = self.db_pool.as_ref().unwrap();
        let pairs_count = pairs.len();
        for (req, resp) in pairs.iter() {
            match req.kind {
                Kind::Activation => {
                    store_activation_response(ProxyMode::Cache, out_pool, req, resp)
                        .await?;
                }
                Kind::Deactivation => {
                    store_deactivation_response(ProxyMode::Cache, out_pool, req, resp)
                        .await?;
                }
            }
        }
        out_pool.close().await;
        eprintln!(
            "Imported {} forwarded request/response pair(s) from {}",
            pairs_count, path
        );
        Ok(())
    }

    pub async fn export(&self, path: &str) -> Result<()> {
        if std::fs::metadata(path).is_ok() {
            return Err(eyre!("Cannot export to an existing file: {}", path));
        }
        // first read the unanswered requests
        let in_pool = self.db_pool.as_ref().unwrap();
        let requests = fetch_unanswered_requests(in_pool).await?;
        in_pool.close().await;
        // now store them to the other database
        let request_count = requests.len();
        let out_pool = db_init(path, "rwc").await?;
        for req in requests.iter() {
            match req.kind {
                Kind::Activation => store_activation_request(&out_pool, req).await?,
                Kind::Deactivation => store_deactivation_request(&out_pool, req).await?,
            }
        }
        out_pool.close().await;
        eprintln!("Exported {} stored request(s) to {}", request_count, path);
        Ok(())
    }

    pub async fn store_request(&self, req: &CRequest) {
        if !self.enabled {
            return;
        }
        let pool = self.db_pool.as_ref().unwrap();
        if let Err(err) = match req.kind {
            Kind::Activation => store_activation_request(pool, req).await,
            Kind::Deactivation => store_deactivation_request(pool, req).await,
        } {
            error!("Cache of {} request {} failed: {:?}", req.kind, req.request_id, err);
        }
    }

    pub async fn store_response(&self, req: &CRequest, resp: &CResponse) {
        if !self.enabled {
            return;
        }
        let pool = self.db_pool.as_ref().unwrap();
        let mode = self.mode.clone();
        if let Err(err) = match req.kind {
            Kind::Activation => store_activation_response(mode, pool, req, resp).await,
            Kind::Deactivation => {
                store_deactivation_response(mode, pool, req, resp).await
            }
        } {
            error!("Cache of {} response {} failed: {:?}", req.kind, req.request_id, err);
        }
    }

    pub async fn fetch_response(&self, req: &CRequest) -> Option<CResponse> {
        if !self.enabled {
            return None;
        }
        let pool = self.db_pool.as_ref().unwrap();
        match match req.kind {
            Kind::Activation => fetch_activation_response(pool, req).await,
            Kind::Deactivation => fetch_deactivation_response(pool, req).await,
        } {
            Ok(resp) => resp,
            Err(err) => {
                error!(
                    "Fetch of {} response {} failed: {:?}",
                    req.kind, req.request_id, err
                );
                None
            }
        }
    }

    pub async fn fetch_forwarding_requests(&self) -> Vec<CRequest> {
        if !self.enabled {
            return Vec::new();
        }
        let pool = self.db_pool.as_ref().unwrap();
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

async fn store_activation_request(pool: &SqlitePool, req: &CRequest) -> Result<()> {
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
    let a_key = activation_id(req);
    debug!("Storing activation request {} with key: {}", &req.request_id, &a_key);
    let mut tx = pool.begin().await?;
    let result = sqlx::query(&i_str)
        .bind(&a_key)
        .bind(deactivation_id(req))
        .bind(&req.api_key)
        .bind(&req.request_id)
        .bind(&req.session_id)
        .bind(&req.device_date)
        .bind(&req.package_id)
        .bind(&req.asnp_id)
        .bind(&req.device_id)
        .bind(&req.os_user_id)
        .bind(req.is_vdi)
        .bind(req.is_domain_user)
        .bind(req.is_virtual)
        .bind(&req.os_name)
        .bind(&req.os_version)
        .bind(&req.app_id)
        .bind(&req.app_version)
        .bind(&req.ngl_version)
        .bind(&req.timestamp)
        .execute(&mut tx)
        .await?;
    tx.commit().await?;
    debug!("Stored activation request has rowid {}", result.last_insert_rowid());
    Ok(())
}

async fn store_deactivation_request(pool: &SqlitePool, req: &CRequest) -> Result<()> {
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
    let d_key = deactivation_id(req);
    debug!("Storing deactivation request {} with key: {}", &req.request_id, &d_key);
    let mut tx = pool.begin().await?;
    let result = sqlx::query(&i_str)
        .bind(&d_key)
        .bind(&req.api_key)
        .bind(&req.request_id)
        .bind(&req.package_id)
        .bind(&req.device_id)
        .bind(&req.os_user_id)
        .bind(req.is_vdi)
        .bind(req.is_domain_user)
        .bind(req.is_virtual)
        .bind(&req.timestamp)
        .execute(&mut tx)
        .await?;
    tx.commit().await?;
    debug!("Stored deactivation request has rowid {}", result.last_insert_rowid());
    Ok(())
}

async fn store_activation_response(
    mode: ProxyMode, pool: &SqlitePool, req: &CRequest, resp: &CResponse,
) -> Result<()> {
    let field_list = "(activation_key, deactivation_key, body, timestamp)";
    let value_list = "(?, ?, ?, ?)";
    let i_str = format!(
        "insert or replace into activation_responses {} values {}",
        field_list, value_list
    );
    let a_key = activation_id(req);
    let d_key = deactivation_id(req);
    let mut tx = pool.begin().await?;
    debug!("Storing activation response {} with key: {}", &req.request_id, &a_key);
    let result = sqlx::query(&i_str)
        .bind(&a_key)
        .bind(&d_key)
        .bind(std::str::from_utf8(&resp.body).unwrap())
        .bind(&req.timestamp)
        .execute(&mut tx)
        .await?;
    debug!("Stored activation response has rowid {}", result.last_insert_rowid());
    if let ProxyMode::Forward = mode {
        // if we are forwarding, then we just remember the response
    } else {
        // otherwise we remove all stored deactivation requests/responses as they are now invalid
        let d_key = deactivation_id(req);
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
    mode: ProxyMode, pool: &SqlitePool, req: &CRequest, resp: &CResponse,
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
        let d_key = deactivation_id(req);
        debug!("Storing deactivation response {} with key: {}", &req.request_id, &d_key);
        let result = sqlx::query(&i_str)
            .bind(&d_key)
            .bind(std::str::from_utf8(&resp.body).unwrap())
            .bind(&req.timestamp)
            .execute(&mut tx)
            .await?;
        debug!("Stored deactivation response has rowid {}", result.last_insert_rowid());
    } else {
        // when we are live, we remove all activation requests/responses as they are now invalid
        let d_key = deactivation_id(req);
        debug!("Removing activation requests with deactivation key: {}", d_key);
        let d_str = "delete from activation_requests where deactivation_key = ?";
        sqlx::query(d_str).bind(&d_key).execute(&mut tx).await?;
        debug!("Removing activation responses with deactivation key: {}", d_key);
        let d_str = "delete from activation_responses where deactivation_key = ?";
        sqlx::query(d_str).bind(&d_key).execute(&mut tx).await?;
        // Remove any pending deactivation requests & responses as they have been completed.
        debug!("Removing deactivation requests with key: {}", d_key);
        let d_str = "delete from deactivation_requests where deactivation_key = ?";
        sqlx::query(&d_str).bind(&d_key).execute(&mut tx).await?;
        debug!("Removing deactivation responses with key: {}", d_key);
        let d_str = "delete from deactivation_responses where deactivation_key = ?";
        sqlx::query(&d_str).bind(&d_key).execute(&mut tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn fetch_activation_response(
    pool: &SqlitePool, req: &CRequest,
) -> Result<Option<CResponse>> {
    let a_key = activation_id(req);
    let q_str =
        "select body, timestamp from activation_responses where activation_key = ?";
    debug!("Finding activation response with key: {}", &a_key);
    let result = sqlx::query(&q_str).bind(&a_key).fetch_optional(pool).await?;
    match result {
        Some(row) => {
            let body: String = row.get("body");
            let timestamp: String = row.get("timestamp");
            Ok(Some(CResponse {
                kind: req.kind.clone(),
                request_id: req.request_id.clone(),
                timestamp,
                body: body.into_bytes(),
            }))
        }
        None => {
            debug!("No activation response found for key: {}", &a_key);
            Ok(None)
        }
    }
}

async fn fetch_deactivation_response(
    pool: &SqlitePool, req: &CRequest,
) -> Result<Option<CResponse>> {
    let a_key = activation_id(req);
    let q_str = "select body from activation_responses where activation_key = ?";
    debug!("Finding deactivation response with key: {}", &a_key);
    let result = sqlx::query(&q_str).bind(&a_key).fetch_optional(pool).await?;
    match result {
        Some(row) => {
            let body: String = row.get("body");
            let timestamp: String = row.get("timestamp");
            Ok(Some(CResponse {
                kind: req.kind.clone(),
                request_id: req.request_id.clone(),
                timestamp,
                body: body.into_bytes(),
            }))
        }
        None => {
            debug!("No deactivation response found for key: {}", &a_key);
            Ok(None)
        }
    }
}

async fn fetch_unanswered_requests(pool: &SqlitePool) -> Result<Vec<CRequest>> {
    let mut activations = fetch_unanswered_activations(pool).await?;
    let mut deactivations = fetch_unanswered_deactivations(pool).await?;
    activations.append(&mut deactivations);
    activations.sort_unstable_by(|r1, r2| r1.timestamp.cmp(&r2.timestamp));
    Ok(activations)
}

async fn fetch_unanswered_activations(pool: &SqlitePool) -> Result<Vec<CRequest>> {
    let mut result: Vec<CRequest> = Vec::new();
    let q_str = r#"select * from activation_requests req where not exists
                    (select 1 from activation_responses where
                        activation_key = req.activation_key and
                        timestamp >= req.timestamp
                    )"#;
    let rows = sqlx::query(q_str).fetch_all(pool).await?;
    for row in rows.iter() {
        result.push(request_from_activation_row(row))
    }
    Ok(result)
}

async fn fetch_unanswered_deactivations(pool: &SqlitePool) -> Result<Vec<CRequest>> {
    let mut result: Vec<CRequest> = Vec::new();
    let q_str = r#"select * from deactivation_requests"#;
    let rows = sqlx::query(q_str).fetch_all(pool).await?;
    for row in rows.iter() {
        result.push(request_from_deactivation_row(row))
    }
    Ok(result)
}

async fn fetch_forwarded_pairs(pool: &SqlitePool) -> Result<Vec<(CRequest, CResponse)>> {
    let mut activations = fetch_forwarded_activations(pool).await?;
    let mut deactivations = fetch_forwarded_deactivations(pool).await?;
    activations.append(&mut deactivations);
    activations.sort_unstable_by(|r1, r2| r1.0.timestamp.cmp(&r2.0.timestamp));
    Ok(activations)
}

async fn fetch_forwarded_activations(
    pool: &SqlitePool,
) -> Result<Vec<(CRequest, CResponse)>> {
    let mut result: Vec<(CRequest, CResponse)> = Vec::new();
    let q_str = r#"
        select req.*, resp.body from activation_requests req 
            inner join activation_responses resp
            on req.activation_key = resp.activation_key"#;
    let rows = sqlx::query(q_str).fetch_all(pool).await?;
    for row in rows.iter() {
        result.push((request_from_activation_row(row), response_from_activation_row(row)))
    }
    Ok(result)
}

async fn fetch_forwarded_deactivations(
    pool: &SqlitePool,
) -> Result<Vec<(CRequest, CResponse)>> {
    let mut result: Vec<(CRequest, CResponse)> = Vec::new();
    let q_str = r#"
        select req.*, resp.body from deactivation_requests req 
            inner join deactivation_responses resp
            on req.deactivation_key = resp.deactivation_key"#;
    let rows = sqlx::query(q_str).fetch_all(pool).await?;
    for row in rows.iter() {
        result.push((
            request_from_deactivation_row(row),
            response_from_deactivation_row(row),
        ));
    }
    Ok(result)
}

fn activation_id(req: &CRequest) -> String {
    let factors: Vec<String> =
        vec![req.app_id.clone(), req.ngl_version.clone(), deactivation_id(req)];
    factors.join("|")
}

fn deactivation_id(req: &CRequest) -> String {
    let factors: Vec<&str> = vec![
        req.package_id.as_str(),
        if req.is_vdi { req.os_user_id.as_str() } else { req.device_id.as_str() },
    ];
    factors.join("|")
}

fn request_from_activation_row(row: &SqliteRow) -> CRequest {
    CRequest {
        kind: Kind::Activation,
        api_key: row.get("api_key"),
        request_id: row.get("request_id"),
        session_id: row.get("session_id"),
        package_id: row.get("package_id"),
        asnp_id: row.get("asnp_id"),
        device_id: row.get("device_id"),
        device_date: row.get("device_date"),
        is_vdi: row.get("is_vdi"),
        is_virtual: row.get("is_virtual"),
        os_name: row.get("os_name"),
        os_version: row.get("os_version"),
        os_user_id: row.get("os_user_id"),
        is_domain_user: row.get("is_domain_user"),
        app_id: row.get("app_id"),
        app_version: row.get("app_version"),
        ngl_version: row.get("ngl_version"),
        timestamp: row.get("timestamp"),
    }
}

fn request_from_deactivation_row(row: &SqliteRow) -> CRequest {
    CRequest {
        kind: Kind::Deactivation,
        api_key: row.get("api_key"),
        request_id: row.get("request_id"),
        package_id: row.get("package_id"),
        device_id: row.get("device_id"),
        is_vdi: row.get("is_vdi"),
        is_virtual: row.get("is_virtual"),
        os_user_id: row.get("os_user_id"),
        is_domain_user: row.get("is_domain_user"),
        timestamp: row.get("timestamp"),
        ..Default::default()
    }
}

fn response_from_activation_row(row: &SqliteRow) -> CResponse {
    let body: String = row.get("body");
    CResponse {
        kind: Kind::Activation,
        request_id: row.get("request_id"),
        body: body.into_bytes(),
        timestamp: row.get("timestamp"),
    }
}

fn response_from_deactivation_row(row: &SqliteRow) -> CResponse {
    let body: String = row.get("body");
    CResponse {
        kind: Kind::Deactivation,
        request_id: row.get("request_id"),
        body: body.into_bytes(),
        timestamp: row.get("timestamp"),
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
