use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::SystemTime;

use base64;
use hyper::{HeaderMap, Uri};
use log::{error, info};
use seahash::SeaHasher;
use serde_json::{from_str, to_string, Value};
use sqlx::sqlite::{SqlitePool, SqliteQueryAs};
use url::Url;

use crate::settings::Settings;

pub struct Cache {
    enabled: bool,
    db_name: Option<String>,
    db_pool: Option<SqlitePool>,
}

#[derive(Hash)]
struct ActivationData {
    package_id: String,
    app_id: String,
    ngl_lib_version: String,
    machine_or_user_id: String,
    os_name: String,
}

const ACTIVATION_SCHEMA: &str = r#"
    create table if not exists activations (
        id integer primary key,
        package_id text not null,
        app_id text not null,
        ngl_lib_version text not null
        machine_or_user_id text not null,
        os_name text not null,
        unique (package_id, app_id, ngl_lib_version, machine_or_user_id, os_name)
    );
    create index if not exists deactivation_index on activations (
        package_id, machine_or_user_id, os_name
    );
    "#;

struct ActivationRequest {
    headers: String,
    body: String,
    timestamp: i64,
}

struct DeactivationRequest {
    query_string: String,
    timestamp: i64,
}

const REQUEST_SCHEMA: &str =
    r#"create table if not exists requests (
        id integer primary key,
        activation_key unique references activations(id),
        timestamp integer not null,
        method text not null
        uri text not null,
        headers text not null,
        body text not null,
        type text not null,
    )"#;

struct ResponseData {
    status_code: i16,
    status_line: String,
    headers: String,
    body: String,
    timestamp: i64,
    response_type: RequestType,
}

const RESPONSE_SCHEMA: &str =
    r#"create table if not exists responses (
        id integer primary key,
        activation_key unique references activations(id),
        timestamp integer not null,
        status_line text not null,
        headers text not null,
        body text not null
    )"#;

const CLEAR_ALL: &str = r#"
    delete from responses;
    delete from requests;
    delete from activations;
    "#;

impl Cache {
    pub async fn from_conf(conf: &Settings) -> Result<Cache, sqlx::Error> {
        if let None | Some(false) = &conf.cache.enabled {
            return Ok(Cache { enabled: false, db_name: None, db_pool: None });
        }
        // make sure the database exists
        let db_name = conf.cache.cache_file_path.as_ref().unwrap().to_string();
        let pool = SqlitePool::builder()
            .max_size(5)
            .build(format!("sqlite:{}", db_name).as_str()).await?;
        sqlx::query(ACTIVATION_SCHEMA).execute(&pool).await?;
        sqlx::query(REQUEST_SCHEMA).execute(&pool).await?;
        sqlx::query(RESPONSE_SCHEMA).execute(&pool).await?;
        info!("Valid cache enabled at '{}'", &db_name);
        Ok(Cache { enabled: true, db_name: Some(db_name), db_pool: Some(pool) })
    }

    pub async fn control(
        &self,
        clear: Option<bool>,
        export_file: Option<String>,
        import_file: Option<String>,
    ) -> Result<(), sqlx::Error> {
        if !self.enabled {
            eprintln!("cache-control: Cache is not enabled");
            std::process::exit(1);
        }
        println!("cache-control: Cache is valid.");
        let db_name = self.db_name.as_ref().unwrap().to_string();
        let pool = self.db_pool.as_ref().unwrap();
        if let Some(_) = import_file {
            eprintln!("cache-control: Import of cache not yet supported, sorry");
        }
        if let Some(_) = export_file {
            eprintln!("cache-control: Export of cache not yet supported, sorry");
        }
        if let Some(clear) = clear {
            if clear {
                sqlx::query(CLEAR_ALL).execute(pool).await?;
                println!("cache-control: Cache has been cleared.");
                info!("Cache at '{}' has been cleared", db_name);
            } else {
                eprintln!("cache-control: To clear the cache, use --clear yes");
            }
        }
        Ok(())
    }

    pub async fn save_request(
        &self, method: &str, uri: &Uri, headers: &HeaderMap, body: &str
    ) -> i64 {
        if !self.enabled {
            return 0;
        }
        if !method.eq_ignore_ascii_case("POST") && !method.eq_ignore_ascii_case("DELETE") {
            return 0;
        }
        let uri = uri.to_string();
        let (uri, uri_key) = uri_string_and_hash(uri);
        let headers = pickle_headers(headers);
        let (body, body_key) = body_string_and_hash(body);
        let key = if method.eq_ignore_ascii_case("POST") { body_key } else { uri_key };
        let key_str = b64encode(&key.to_le_bytes().to_vec());
        let uri_str = b64encode(&uri);
        let header_str = b64encode(&headers);
        let body_str = b64encode(&body);
        let timestamp = chrono::offset::Utc::now().timestamp();
        let pool = self.db_pool.as_ref().unwrap();
        let mut tx = pool.begin()
            .await.expect("Can't begin cache transaction");
        sqlx::query!(r#"
            replace into requests (timestamp, activation_key, method, uri, headers, body)
            values (?, ?, ?, ?, ?);
            "#, timestamp, &key_str, method, &uri_str, &header_str, &body_str)
            .execute(&mut tx)
            .await.expect("Can't insert or replace request in cache");
        let result: (i64, ) = sqlx::query_as("select last_insert_rowid()")
            .fetch_one(&mut tx)
            .await.expect("Lookup of inserted request row failed");
        result.0
    }
}

#[derive(Hash)]
struct BodyKey {
    package_id: String,
    app_id: String,
    app_version: String,
    ngl_lib_version: String,
    machine_id: String,
    os_name: String,
    os_user_id: String,
}

#[derive(Hash)]
struct UriKey {
    package_id: String,
    machine_id: String,
}

fn uri_hash(url: &str) -> u64 {
    let mut machine_id = "no-machine-id".to_string();
    let mut package_id = "no-package-id".to_string();
    for (k, v) in Url::parse(url).unwrap().query_pairs() {
        if k.eq_ignore_ascii_case("deviceId") {
            machine_id = v.to_string();
        } else if k.eq_ignore_ascii_case("npdId") {
            package_id = v.to_string();
        }
    }
    let key = UriKey { package_id, machine_id };
    let mut hasher = SeaHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

fn headers_to_string(header_map: &HeaderMap) -> String {
    let headers = serde_json::map::Map::new();
    for (k, v) in header_map.iter() {
        let k_str = k.as_str().to_string();
        let v_str = v.to_str().unwrap().to_string();
        if headers.contains_key(&k_str) {
            error!("Request contains multiple '{}' headers; ignoring all but first", &k_str);
            continue;
        }
        headers[k_str] = v_str;
    }
    let string = serde_json::to_string(&headers).unwrap();
    debug!("Cached request headers: {}", string);
    string
}

fn body_hashes(body_str: &str) -> (u64, u64) {
    let body: Value = from_str(body_str).expect("Can't parse request body");
    let package_id = string_or(&body["npdId"], "no-package-ID");
    let app_id = string_or(&body["appDetails"]["nglAppId"], "no-app-ID");
    let ngl_lib_version = string_or(&body["appDetails"]["nglLibVersion"], "no-lib-version");
    let machine_id = string_or(&body["deviceDetails"]["deviceId"], "no-device-ID");
    let os_name = string_or(&body["deviceDetails"]["osName"], "no-OS-name");
    let os_user_id = string_or(&body["deviceDetails"]["osUserId"], "no-OS-user");
    let key = UriKey { package_id, machine_id };
    let mut uri_hasher = SeaHasher::new();
    key.hash(&mut uri_hasher);
    let key = BodyKey { package_id, app_id, ngl_lib_version, machine_id, os_name, os_user_id };
    let mut body_hasher = SeaHasher::new();
    key.hash(&mut body_hasher);
    (uri_hasher.finish(), body_hasher.finish())
}

fn string_or(v: &Value, default: &str) -> String {
    if v.is_string() {
        to_string(v).unwrap()
    } else {
        default.to_string()
    }
}

pub fn b64encode(v: &Vec<u8>) -> String {
    base64::encode_config(v.as_ref(), base64::URL_SAFE_NO_PAD).unwrap()
}

pub fn b64decode(s: &str) -> Vec<u8> {
    base64::decode_config(s, base64::URL_SAFE_NO_PAD).unwrap()
}

