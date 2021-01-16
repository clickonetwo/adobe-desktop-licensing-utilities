/*
Copyright 2020 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
use crate::cops::{Kind, Request as CRequest, Response as CResponse};
use crate::settings::Settings;
use dialoguer::Confirm;
use log::{debug, error, info};
use sqlx::{
    sqlite::{SqlitePool, SqlitePoolOptions},
    Row,
};
use std::sync::Arc;

#[derive(Default)]
pub struct Cache {
    enabled: bool,
    db_name: Option<String>,
    db_pool: Option<SqlitePool>,
}

impl Cache {
    pub async fn new_from(conf: &Settings) -> Result<Arc<Cache>, sqlx::Error> {
        if conf.proxy.mode.starts_with('p') {
            return Ok(Arc::new(Cache::default()));
        }
        // make sure the database exists
        let db_name = conf.cache.cache_file_path.as_ref().unwrap().to_string();
        let db_url = format!("file:{}?mode=rwc", db_name);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(db_url.as_str())
            .await?;
        sqlx::query(ACTIVATION_REQUEST_SCHEMA)
            .execute(&pool)
            .await?;
        sqlx::query(DEACTIVATION_REQUEST_SCHEMA)
            .execute(&pool)
            .await?;
        sqlx::query(ACTIVATION_RESPONSE_SCHEMA)
            .execute(&pool)
            .await?;
        info!("Valid cache enabled at '{}'", &db_name);
        Ok(Arc::new(Cache {
            enabled: true,
            db_name: Some(db_name),
            db_pool: Some(pool),
        }))
    }

    pub async fn control(
        &self, clear: bool, yes: bool, export_file: Option<String>,
        import_file: Option<String>,
    ) -> Result<(), sqlx::Error> {
        if !self.enabled {
            eprintln!("cache-control: Cache is not enabled");
            std::process::exit(1);
        }
        println!("cache-control: Cache is valid.");
        let db_name = self.db_name.as_ref().unwrap().to_string();
        let pool = self.db_pool.as_ref().unwrap();
        if let Some(path) = import_file {
            println!("cache-control: Requesting import of cache from '{}'", path);
            eprintln!("cache-control: Import of cache not yet supported, sorry");
        }
        if let Some(path) = export_file {
            println!("cache-control: Requesting export of cache to '{}'", path);
            eprintln!("cache-control: Export of cache not yet supported, sorry");
        }
        if clear {
            let confirm = match yes {
                true => true,
                false => Confirm::new()
                    .with_prompt(
                        "Really clear the cache? This operation cannot be undone.",
                    )
                    .default(false)
                    .show_default(true)
                    .interact()
                    .unwrap(),
            };
            if confirm {
                sqlx::query(CLEAR_ALL).execute(pool).await?;
                println!("cache-control: Cache has been cleared.");
                info!("Cache at '{}' has been cleared", db_name);
            }
        }
        Ok(())
    }

    pub async fn store_request(&self, req: &CRequest) {
        if !self.enabled {
            return;
        }
        match req.kind {
            Kind::Activation => self.store_activation_request(req).await,
            Kind::Deactivation => self.store_deactivation_request(req).await,
        }
    }

    async fn store_activation_request(&self, req: &CRequest) {
        let pool = self
            .db_pool
            .as_ref()
            .expect("Invoke of cache while disabled.");
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
        debug!(
            "Storing activation request {} with key: {}",
            &req.request_id, &a_key
        );
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
            .await;
        match result {
            Ok(done) => debug!(
                "Stored activation request has rowid {}",
                done.last_insert_rowid()
            ),
            Err(err) => error!("Cache store of activation request failed: {:?}", err),
        }
    }

    async fn store_deactivation_request(&self, req: &CRequest) {
        let pool = self
            .db_pool
            .as_ref()
            .expect("Invoke of cache while disabled.");
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
        debug!(
            "Storing deactivation request {} with key: {}",
            &req.request_id, &d_key
        );
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
            .await;
        match result {
            Ok(done) => debug!(
                "Stored deactivation request has rowid {}",
                done.last_insert_rowid()
            ),
            Err(err) => error!("Cache store of deactivation request failed: {:?}", err),
        }
    }

    pub async fn store_response(&self, req: &CRequest, resp: &CResponse) {
        if !self.enabled {
            return;
        }
        match req.kind {
            Kind::Activation => self.store_activation_response(req, resp).await,
            Kind::Deactivation => self.store_deactivation_response(req, resp).await,
        }
    }

    async fn store_activation_response(&self, req: &CRequest, resp: &CResponse) {
        let pool = self
            .db_pool
            .as_ref()
            .expect("Invoke of cache while disabled.");
        let field_list = "(activation_key, deactivation_key, body, timestamp)";
        let value_list = "(?, ?, ?, ?)";
        let i_str = format!(
            "insert or replace into activation_responses {} values {}",
            field_list, value_list
        );
        let a_key = activation_id(req);
        let d_key = deactivation_id(req);
        debug!(
            "Storing activation response {} with key: {}",
            &req.request_id, &a_key
        );
        let result = sqlx::query(&i_str)
            .bind(&a_key)
            .bind(&d_key)
            .bind(std::str::from_utf8(&resp.body).unwrap())
            .bind(&req.timestamp)
            .execute(pool)
            .await;
        match result {
            Ok(done) => debug!(
                "Stored activation response has rowid {}",
                done.last_insert_rowid()
            ),
            Err(err) => error!("Cache store of activation response failed: {:?}", err),
        }
        // when we see a live activation, we remove any stored deactivation requests
        // that are superseded by this later activation, so they won't be later forwarded.
        let d_key = deactivation_id(req);
        debug!("Removing deactivation requests with key: {}", d_key);
        let d_str = "delete from deactivation_requests where deactivation_key = ?";
        let result = sqlx::query(d_str).bind(&d_key).execute(pool).await;
        if let Err(err) = result {
            error!("Cache delete of deactivation requests failed: {:?}", err);
        }
    }

    async fn store_deactivation_response(&self, req: &CRequest, _resp: &CResponse) {
        let pool = self
            .db_pool
            .as_ref()
            .expect("Invoke of cache while disabled.");
        // when we get a live deactivation, we remove all the activation request/response pairs
        // this response deactivates, so earlier requests are no longer stored for forwarding
        // and later requests will not receive a cached response.
        let d_key = deactivation_id(req);
        debug!(
            "Removing activation requests with deactivation key: {}",
            d_key
        );
        let d_str = "delete from activation_requests where deactivation_key = ?";
        let result = sqlx::query(d_str).bind(&d_key).execute(pool).await;
        if let Err(err) = result {
            error!("Cache delete of activation requests failed: {:?}", err);
        }
        debug!(
            "Removing activation responses with deactivation key: {}",
            d_key
        );
        let d_str = "delete from activation_responses where deactivation_key = ?";
        let result = sqlx::query(d_str).bind(&d_key).execute(pool).await;
        if let Err(err) = result {
            error!("Cache delete of activation responses failed: {:?}", err);
        }
        // Since we don't store this response, its request would later be forwarded again,
        // so we remove any pending requests to avoid their being made again.
        debug!("Removing deactivation requests with key: {}", d_key);
        let d_str = "delete from deactivation_requests where deactivation_key = ?";
        let result = sqlx::query(&d_str).bind(&d_key).execute(pool).await;
        if let Err(err) = result {
            error!("Cache delete of deactivation requests failed: {:?}", err);
        }
    }

    pub async fn fetch_response(&self, req: &CRequest) -> Option<CResponse> {
        if !self.enabled {
            return None;
        }
        match req.kind {
            Kind::Activation => self.fetch_activation_response(req).await,
            Kind::Deactivation => self.fetch_deactivation_response(req).await,
        }
    }

    async fn fetch_activation_response(&self, req: &CRequest) -> Option<CResponse> {
        let pool = self
            .db_pool
            .as_ref()
            .expect("Invoke of cache while disabled.");
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

    async fn fetch_deactivation_response(&self, req: &CRequest) -> Option<CResponse> {
        let pool = self
            .db_pool
            .as_ref()
            .expect("Invoke of cache while disabled.");
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

    pub async fn fetch_stored_requests(&self) -> Vec<CRequest> {
        let mut activations = self.fetch_stored_activations().await;
        let mut deactivations = self.fetch_stored_deactivations().await;
        activations.append(&mut deactivations);
        activations.sort_unstable_by(|r1, r2| r1.timestamp.cmp(&r2.timestamp));
        activations
    }

    async fn fetch_stored_activations(&self) -> Vec<CRequest> {
        let mut result: Vec<CRequest> = Vec::new();
        let pool = self
            .db_pool
            .as_ref()
            .expect("Invoke of cache while disabled.");
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

    async fn fetch_stored_deactivations(&self) -> Vec<CRequest> {
        let mut result: Vec<CRequest> = Vec::new();
        let pool = self
            .db_pool
            .as_ref()
            .expect("Invoke of cache while disabled.");
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
}

fn activation_id(req: &CRequest) -> String {
    let factors: Vec<String> = vec![
        req.app_id.clone(),
        req.ngl_version.clone(),
        deactivation_id(req),
    ];
    factors.join("|")
}

fn deactivation_id(req: &CRequest) -> String {
    let factors: Vec<&str> = vec![
        req.package_id.as_str(),
        if req.is_vdi {
            req.os_user_id.as_str()
        } else {
            req.device_id.as_str()
        },
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

const CLEAR_ALL: &str = r#"
    delete from deactivation_requests;
    delete from activation_responses;
    delete from activation_requests;
    "#;
