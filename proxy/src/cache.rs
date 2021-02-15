/*
Copyright 2020 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
use crate::cops::{Kind, Request as CRequest, Response as CResponse, Response};
use crate::settings::{ProxyMode, Settings};
use dialoguer::Confirm;
use eyre::{eyre, Result, WrapErr};
use log::{debug, error, info};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions},
    ConnectOptions, Row,
};
use std::{env, str::FromStr, sync::Arc};

#[derive(Default)]
pub struct Cache {
    enabled: bool,
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
            std::fs::metadata(db_name)
                .wrap_err(format!("Can't access cache db: {}", db_name))?;
            "rw"
        };
        let pool = db_init(db_name, mode)
            .await
            .wrap_err(format!("Can't connect to cache db: {}", db_name))?;
        info!("Valid cache database: {}", &db_name);
        Ok(Arc::new(Cache { enabled: true, db_pool: Some(pool) }))
    }

    pub async fn clear(&self, yes: bool) -> Result<()> {
        let pool = self.db_pool.as_ref().unwrap();
        let confirm = match yes {
            true => true,
            false => Confirm::new()
                .with_prompt("Really clear the cache? This operation cannot be undone.")
                .default(false)
                .show_default(true)
                .interact()?,
        };
        if confirm {
            sqlx::query(CLEAR_ALL)
                .execute(pool)
                .await
                .wrap_err("Failed to clear cache")?;
            eprintln!("Cache has been cleared.");
        }
        Ok(())
    }

    pub async fn import(&self, path: &str) -> Result<()> {
        std::fs::metadata(path).wrap_err(format!("Cannot import: {}", path))?;
        let in_pool = db_init(path, "ro").await.wrap_err("Can't open import database")?;
        let pairs = fetch_forwarded_pairs(&in_pool).await;
        in_pool.close().await;
        let pool = self.db_pool.as_ref().unwrap();
        let pairs_count = pairs.len();
        for (req, resp) in pairs.iter() {
            match req.kind {
                Kind::Activation => {
                    store_activation_response(pool, req, resp)
                        .await
                        .wrap_err("Failure importing forwarded activation")?;
                    process_activation_response(pool, req)
                        .await
                        .wrap_err("Failure processing forwareded activation")?;
                }
                Kind::Deactivation => {
                    store_deactivation_response(pool, req, resp)
                        .await
                        .wrap_err("Failure importing forwarded deactivation")?;
                    process_deactivation_response(pool, req)
                        .await
                        .wrap_err("Failure processing forwarded deactivation")?;
                }
            }
        }
        eprintln!("Imported {} forwarded requests from database: {}", pairs_count, path);
        Ok(())
    }

    pub async fn export(&self, path: &str) -> Result<()> {
        if std::fs::metadata(path).is_ok() {
            return Err(eyre!("Cannot export to an existing file: {}", path));
        }
        let in_pool = self.db_pool.as_ref().unwrap();
        let requests = fetch_unanswered_requests(in_pool).await;
        in_pool.close().await;
        let request_count = requests.len();
        let out_pool =
            db_init(path, "rwc").await.wrap_err("Cannot initialize export database")?;
        for req in requests.iter() {
            match req.kind {
                Kind::Activation => store_activation_request(&out_pool, req)
                    .await
                    .wrap_err("Failure exporting activation request")?,
                Kind::Deactivation => store_deactivation_request(&out_pool, req)
                    .await
                    .wrap_err("Failure exporting deactivation request")?,
            }
        }
        eprintln!("Exported {} requests to database {}", request_count, path);
        Ok(())
    }

    pub async fn store_request(&self, req: &CRequest) {
        if !self.enabled {
            return;
        }
        let pool = self.db_pool.as_ref().unwrap();
        match req.kind {
            Kind::Activation => {
                if let Err(err) = store_activation_request(pool, req).await {
                    error!("Failed to store activation request: {:?}", err);
                }
            }
            Kind::Deactivation => {
                if let Err(err) = store_deactivation_request(pool, req).await {
                    error!("Failed to store deactivation request: {:?}", err);
                }
            }
        }
    }

    pub async fn store_response(&self, req: &CRequest, resp: &CResponse) {
        if !self.enabled {
            return;
        }
        let pool = self.db_pool.as_ref().unwrap();
        match req.kind {
            Kind::Activation => {
                if let Err(err) = store_activation_response(pool, req, resp).await {
                    error!("Failed to store activation response: {:?}", err)
                }
            }
            Kind::Deactivation => {
                if let Err(err) = store_deactivation_response(pool, req, resp).await {
                    error!("Failed to store deactivation response: {:?}", err)
                }
            }
        }
    }

    pub async fn process_response(&self, req: &CRequest) {
        if !self.enabled {
            return;
        }
        let pool = self.db_pool.as_ref().unwrap();
        match req.kind {
            Kind::Activation => {
                if let Err(err) = process_activation_response(pool, req).await {
                    error!("Failed to process activation response: {:?}", err);
                }
            }
            Kind::Deactivation => {
                if let Err(err) = process_deactivation_response(pool, req).await {
                    error!("Failed to process deactivation response: {:?}", err);
                }
            }
        }
    }

    pub async fn fetch_response(&self, req: &CRequest) -> Option<CResponse> {
        if !self.enabled {
            return None;
        }
        let pool = self.db_pool.as_ref().unwrap();
        match req.kind {
            Kind::Activation => fetch_activation_response(pool, req).await,
            Kind::Deactivation => fetch_deactivation_response(pool, req).await,
        }
    }

    pub async fn fetch_forwarding_requests(&self) -> Vec<CRequest> {
        if !self.enabled {
            return Vec::new();
        }
        let pool = self.db_pool.as_ref().unwrap();
        fetch_unanswered_requests(pool).await
    }
}

async fn db_init(db_name: &str, mode: &str) -> Result<SqlitePool> {
    let db_url = format!("file:{}?mode={}", db_name, mode);
    let mut options = SqliteConnectOptions::from_str(&db_url).map_err(|e| eyre!(e))?;
    if env::var("FRL_PROXY_ENABLE_STATEMENT_LOGGING").is_err() {
        options.disable_statement_logging();
    }
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|e| eyre!(e))?;
    sqlx::query(ACTIVATION_REQUEST_SCHEMA).execute(&pool).await.map_err(|e| eyre!(e))?;
    sqlx::query(DEACTIVATION_REQUEST_SCHEMA)
        .execute(&pool)
        .await
        .map_err(|e| eyre!(e))?;
    sqlx::query(ACTIVATION_RESPONSE_SCHEMA).execute(&pool).await.map_err(|e| eyre!(e))?;
    sqlx::query(DEACTIVATION_RESPONSE_SCHEMA)
        .execute(&pool)
        .await
        .map_err(|e| eyre!(e))?;
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
        .execute(pool)
        .await
        .map_err(|e| eyre!(e))?;
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
        .execute(pool)
        .await
        .map_err(|e| eyre!(e))?;
    debug!("Stored deactivation request has rowid {}", result.last_insert_rowid());
    Ok(())
}

async fn store_activation_response(
    pool: &SqlitePool, req: &CRequest, resp: &CResponse,
) -> Result<()> {
    let field_list = "(activation_key, deactivation_key, body, timestamp)";
    let value_list = "(?, ?, ?, ?)";
    let i_str = format!(
        "insert or replace into activation_responses {} values {}",
        field_list, value_list
    );
    let a_key = activation_id(req);
    let d_key = deactivation_id(req);
    debug!("Storing activation response {} with key: {}", &req.request_id, &a_key);
    let result = sqlx::query(&i_str)
        .bind(&a_key)
        .bind(&d_key)
        .bind(std::str::from_utf8(&resp.body).unwrap())
        .bind(&req.timestamp)
        .execute(pool)
        .await
        .map_err(|e| eyre!(e))?;
    debug!("Stored activation response has rowid {}", result.last_insert_rowid());
    Ok(())
}

async fn store_deactivation_response(
    pool: &SqlitePool, req: &CRequest, resp: &CResponse,
) -> Result<()> {
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
        .execute(pool)
        .await
        .map_err(|e| eyre!(e))?;
    debug!("Stored deactivation response has rowid {}", result.last_insert_rowid());
    Ok(())
}

async fn process_activation_response(pool: &SqlitePool, req: &CRequest) -> Result<()> {
    // remove any stored deactivation requests/responses, as they are now invalid.
    let d_key = deactivation_id(req);
    debug!("Removing deactivation requests with key: {}", d_key);
    let d_str = "delete from deactivation_requests where deactivation_key = ?";
    sqlx::query(d_str).bind(&d_key).execute(pool).await.map_err(|e| eyre!(e))?;
    debug!("Removing deactivation responses with key: {}", d_key);
    let d_str = "delete from deactivation_responses where deactivation_key = ?";
    sqlx::query(d_str).bind(&d_key).execute(pool).await.map_err(|e| eyre!(e))?;
    Ok(())
}

async fn process_deactivation_response(pool: &SqlitePool, req: &CRequest) -> Result<()> {
    // remove any activation requests/responses as they are now invalid
    let d_key = deactivation_id(req);
    debug!("Removing activation requests with deactivation key: {}", d_key);
    let d_str = "delete from activation_requests where deactivation_key = ?";
    sqlx::query(d_str).bind(&d_key).execute(pool).await.map_err(|e| eyre!(e))?;
    debug!("Removing activation responses with deactivation key: {}", d_key);
    let d_str = "delete from activation_responses where deactivation_key = ?";
    sqlx::query(d_str).bind(&d_key).execute(pool).await.map_err(|e| eyre!(e))?;
    // Remove any pending deactivation requests & responses as they have been completed.
    debug!("Removing deactivation requests with key: {}", d_key);
    let d_str = "delete from deactivation_requests where deactivation_key = ?";
    sqlx::query(&d_str).bind(&d_key).execute(pool).await.map_err(|e| eyre!(e))?;
    debug!("Removing deactivation responses with key: {}", d_key);
    let d_str = "delete from deactivation_responses where deactivation_key = ?";
    sqlx::query(&d_str).bind(&d_key).execute(pool).await.map_err(|e| eyre!(e))?;
    Ok(())
}

async fn fetch_activation_response(
    pool: &SqlitePool, req: &CRequest,
) -> Option<CResponse> {
    let a_key = activation_id(req);
    let q_str =
        "select body, timestamp from activation_responses where activation_key = ?";
    debug!("Finding activation response with key: {}", &a_key);
    let result = sqlx::query(&q_str).bind(&a_key).fetch_optional(pool).await;
    match result {
        Ok(Some(row)) => {
            let body: String = row.get("body");
            let timestamp: String = row.get("timestamp");
            Some(CResponse {
                kind: req.kind.clone(),
                request_id: req.request_id.clone(),
                timestamp,
                body: body.into_bytes(),
            })
        }
        Ok(None) => {
            debug!("No activation response found for key: {}", &a_key);
            None
        }
        Err(err) => {
            debug!("Error during fetch of activation response: {:?}", err);
            None
        }
    }
}

async fn fetch_deactivation_response(
    pool: &SqlitePool, req: &CRequest,
) -> Option<CResponse> {
    let a_key = activation_id(req);
    let q_str = "select body from activation_responses where activation_key = ?";
    debug!("Finding deactivation response with key: {}", &a_key);
    let result = sqlx::query(&q_str).bind(&a_key).fetch_optional(pool).await;
    if let Ok(Some(row)) = result {
        let body: String = row.get("body");
        let timestamp: String = row.get("timestamp");
        Some(CResponse {
            kind: req.kind.clone(),
            request_id: req.request_id.clone(),
            timestamp,
            body: body.into_bytes(),
        })
    } else {
        if let Err(err) = result {
            debug!("Error during fetch of deactivation response: {:?}", err);
        }
        None
    }
}

async fn fetch_unanswered_requests(pool: &SqlitePool) -> Vec<CRequest> {
    let mut activations = fetch_unanswered_activations(pool).await;
    let mut deactivations = fetch_unanswered_deactivations(pool).await;
    activations.append(&mut deactivations);
    activations.sort_unstable_by(|r1, r2| r1.timestamp.cmp(&r2.timestamp));
    activations
}

async fn fetch_unanswered_activations(pool: &SqlitePool) -> Vec<CRequest> {
    let mut result: Vec<CRequest> = Vec::new();
    let q_str = r#"select * from activation_requests req where not exists
                    (select 1 from activation_responses where
                        activation_key = req.activation_key and
                        timestamp >= req.timestamp
                    )"#;
    let rows = sqlx::query(q_str).fetch_all(pool).await;
    if let Err(err) = rows {
        debug!("Error during fetch of activation requests: {:?}", err);
        return result;
    }
    for row in rows.unwrap().iter() {
        result.push(CRequest {
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
        })
    }
    result
}

async fn fetch_unanswered_deactivations(pool: &SqlitePool) -> Vec<CRequest> {
    let mut result: Vec<CRequest> = Vec::new();
    let q_str = r#"select * from deactivation_requests"#;
    let rows = sqlx::query(q_str).fetch_all(pool).await;
    if let Err(err) = rows {
        debug!("Error during fetch of activation requests: {:?}", err);
        return result;
    }
    for row in rows.unwrap().iter() {
        result.push(CRequest {
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
        })
    }
    result
}

async fn fetch_forwarded_pairs(pool: &SqlitePool) -> Vec<(CRequest, CResponse)> {
    let mut activations = fetch_forwarded_activations(pool).await;
    let mut deactivations = fetch_forwarded_deactivations(pool).await;
    activations.append(&mut deactivations);
    activations.sort_unstable_by(|r1, r2| r1.0.timestamp.cmp(&r2.0.timestamp));
    activations
}

async fn fetch_forwarded_activations(pool: &SqlitePool) -> Vec<(CRequest, CResponse)> {
    let mut result: Vec<(CRequest, CResponse)> = Vec::new();
    let q_str = r#"
        select req.*, resp.body from activation_requests req 
            inner join activation_responses resp
            on req.activation_key = resp.activation_key"#;
    let rows = sqlx::query(q_str).fetch_all(pool).await;
    if let Err(err) = rows {
        debug!("Error during fetch of forwarded activations: {:?}", err);
        return result;
    }
    for row in rows.unwrap().iter() {
        let body: String = row.get("body");
        result.push((
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
            },
            CResponse {
                kind: Kind::Activation,
                request_id: row.get("request_id"),
                body: body.into_bytes(),
                timestamp: row.get("timestamp"),
            },
        ))
    }
    result
}

async fn fetch_forwarded_deactivations(pool: &SqlitePool) -> Vec<(CRequest, CResponse)> {
    let mut result: Vec<(CRequest, CResponse)> = Vec::new();
    let q_str = r#"
        select req.*, resp.body from deactivation_requests req 
            inner join deactivation_responses resp
            on req.deactivation_key = resp.deactivation_key"#;
    let rows = sqlx::query(q_str).fetch_all(pool).await;
    if let Err(err) = rows {
        debug!("Error during fetch of forwarded deactivations: {:?}", err);
        return result;
    }
    for row in rows.unwrap().iter() {
        let body: String = row.get("body");
        result.push((
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
            },
            Response {
                kind: Kind::Deactivation,
                request_id: row.get("request_id"),
                body: body.into_bytes(),
                timestamp: row.get("timestamp"),
            },
        ))
    }
    result
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
