use sqlx::{Connect, SqliteConnection};
use log::info;

use crate::settings::Settings;

const REQUEST_SCHEMA: &str =
    r#"create table if not exists "requests" (
        "id" integer primary key,
        "cache_key" string not null unique,
        "method" string not null,
        "machine_id" string not null,
        "package_id" string not null,
        "app_id" string not null,
        "platform_id" string not null,
        "body" string not null
    );"#;
const RESPONSE_SCHEMA: &str =
    r#"create table if not exists "responses" (
        "id" integer primary key,
        "request_id" integer not null references requests(id),
        "body" string not null
    );"#;
const CLEAR_ALL: &str =
    r#"delete from "responses"; delete from "requests";"#;

pub async fn init(conf: &Settings) -> Result<(), sqlx::Error> {
    if let None | Some(false) = &conf.cache.enabled {
        return Ok(());
    }
    // make sure the database exists
    let db_url = format!("sqlite:{}", conf.cache.cache_file_path.as_ref().unwrap());
    let mut conn = SqliteConnection::connect(&db_url).await?;
    sqlx::query(REQUEST_SCHEMA).execute(&mut conn).await?;
    sqlx::query(RESPONSE_SCHEMA).execute(&mut conn).await?;
    info!("Valid cache enabled at '{}'", &db_url);
    Ok(())
}

pub async fn cache_control(
    conf: &Settings, clear: Option<String>, export_file: Option<String>,
) -> Result<(), sqlx::Error> {
    if let None | Some(false) = &conf.cache.enabled {
        eprintln!("Cache must be enabled when using cache-control");
    }
    init(conf).await?;
    println!("cache-control: Cache is valid.");
    if let Some(_) = export_file {
        eprintln!("cache-control: Export of cache not yet supported, sorry");
    }
    if let Some(clear) = clear {
        if clear.eq_ignore_ascii_case("yes") || clear.eq_ignore_ascii_case("true") {
            let db_url = format!("sqlite:{}", &conf.cache.cache_file_path.as_ref().unwrap());
            let mut conn = SqliteConnection::connect(&db_url).await?;
            sqlx::query(CLEAR_ALL).execute(&mut conn).await?;
            println!("cache-control: Cache has been cleared.");
            info!("Cache at '{}' has been cleared", &db_url);
        } else {
            eprintln!("cache-control: To clear the cache, use --clear yes");
        }
    }
    Ok(())
}
