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
use std::env;
use std::str::FromStr;
use std::sync::Arc;

use ::log::{error, info};
use dialoguer::Confirm;
use eyre::{eyre, Result, WrapErr};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions},
    ConnectOptions,
};

use crate::cli::Datasource;
use adlu_parse::protocol::{Request, Response};

mod frl;
mod log;
mod named_user;

/// A cache for requests and responses.
///
/// This cache uses an SQLite v3 database accessed asynchronously via `sqlx`.
pub type Cache = Arc<Db>;

pub async fn connect(path: &str) -> Result<Cache> {
    Ok(Arc::new(Db::from(path).await?))
}

#[derive(Debug)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    async fn from(path: &str) -> Result<Self> {
        let pool = db_init(path, "rwc")
            .await
            .wrap_err(format!("Can't connect to cache db: {}", path))?;
        info!("Valid cache database: {}", &path);
        Ok(Self { pool })
    }

    pub async fn close(&self) {
        self.pool.close().await;
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
            let pool = &self.pool;
            frl::clear(pool).await?;
            log::clear(pool).await?;
            named_user::clear(pool).await?;
        }
        Ok(())
    }

    pub async fn import(&self, source: &Datasource, path: &str) -> Result<()> {
        if let Datasource::Frl = source {
            frl::import(&self.pool, path).await
        } else {
            Err(eyre!("Import of {} is not yet implemented.", &source))
        }
    }

    pub async fn export(&self, source: &Datasource, path: &str) -> Result<()> {
        if let Datasource::Frl = source {
            frl::export(&self.pool, path).await
        } else {
            Err(eyre!("Export of {} is not yet implemented.", &source))
        }
    }

    pub async fn report(
        &self,
        source: &Datasource,
        path: &str,
        empty: bool,
        timezone: bool,
        rfc3339: bool,
    ) -> Result<()> {
        match source {
            Datasource::Frl => {
                Err(eyre!("Reporting of {} is not yet implemented", &source))
            }
            Datasource::Nul => {
                named_user::report(&self.pool, path, empty, timezone, rfc3339).await
            }
            Datasource::Log => {
                log::report(&self.pool, path, empty, timezone, rfc3339).await
            }
        }
    }

    pub async fn store_request(&self, req: &Request) {
        let pool = &self.pool;
        let result = match req {
            Request::FrlActivation(req) => frl::store_activation_request(pool, req).await,
            Request::FrlDeactivation(req) => {
                frl::store_deactivation_request(pool, req).await
            }
            Request::NulActivation(req) => {
                named_user::store_license_request(pool, req).await
            }
            Request::LogUpload(req) => log::store_upload_request(pool, req).await,
        };
        if let Err(err) = result {
            let id = req.request_id();
            error!("Cache store of request ID {} failed: {}", id, err);
        }
    }

    pub async fn store_response(&self, req: &Request, resp: &Response) {
        let pool = &self.pool;
        let mismatch =
            eyre!("Internal request/response inconsistency: please report a bug!");
        let result = match resp {
            Response::FrlActivation(resp) => {
                if let Request::FrlActivation(req) = req {
                    frl::store_activation_response(pool, req, resp).await
                } else {
                    Err(mismatch)
                }
            }
            Response::FrlDeactivation(resp) => {
                if let Request::FrlDeactivation(req) = req {
                    frl::store_deactivation_response(pool, req, resp).await
                } else {
                    Err(mismatch)
                }
            }
            Response::NulActivation(resp) => {
                if let Request::NulActivation(req) = req {
                    named_user::store_license_response(pool, req, resp).await
                } else {
                    Err(mismatch)
                }
            }
            Response::LogUpload(resp) => {
                if let Request::LogUpload(req) = req {
                    log::store_upload_response(pool, req, resp).await
                } else {
                    Err(mismatch)
                }
            }
        };
        if let Err(err) = result {
            error!("Cache store of request ID {} failed: {}", req.request_id(), err);
        }
    }

    pub async fn fetch_response(&self, req: &Request) -> Option<Response> {
        let pool = &self.pool;
        let result = match req {
            Request::FrlActivation(req) => {
                match frl::fetch_activation_response(pool, req).await {
                    Ok(Some(resp)) => Ok(Some(Response::FrlActivation(Box::new(resp)))),
                    Ok(None) => Ok(None),
                    Err(err) => Err(err),
                }
            }
            Request::FrlDeactivation(req) => {
                match frl::fetch_deactivation_response(pool, req).await {
                    Ok(Some(resp)) => Ok(Some(Response::FrlDeactivation(Box::new(resp)))),
                    Ok(None) => Ok(None),
                    Err(err) => Err(err),
                }
            }
            Request::NulActivation(req) => {
                match named_user::fetch_license_response(pool, req).await {
                    Ok(Some(resp)) => Ok(Some(Response::NulActivation(Box::new(resp)))),
                    Ok(None) => Ok(None),
                    Err(err) => Err(err),
                }
            }
            Request::LogUpload(req) => {
                match log::fetch_upload_response(pool, req).await {
                    Ok(Some(resp)) => Ok(Some(Response::LogUpload(Box::new(resp)))),
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

    pub async fn fetch_unanswered_requests(&self) -> Result<Vec<Request>> {
        frl::fetch_unanswered_requests(&self.pool).await
    }
}

async fn db_init(db_name: &str, mode: &str) -> Result<SqlitePool> {
    let db_url = format!("sqlite:{}?mode={}", db_name, mode);
    let mut options: SqliteConnectOptions =
        SqliteConnectOptions::from_str(&db_url).map_err(|e| eyre!(e))?;
    if env::var("ADLU_PROXY_ENABLE_STATEMENT_LOGGING").is_err() {
        options.disable_statement_logging();
    }
    let pool = SqlitePoolOptions::new().max_connections(5).connect_with(options).await?;
    frl::db_init(&pool).await?;
    log::db_init(&pool).await?;
    named_user::db_init(&pool).await?;
    Ok(pool)
}
