use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::Manager;

const DATABASE_NAME: &str = "firelink.sqlite";
const LEGACY_STORE_NAME: &str = "store.bin";
const LEGACY_BUNDLE_IDENTIFIER: &str = "com.nima.tauri-app";
const CURRENT_SCHEMA_VERSION: i64 = 1;
const TOKEN_CHANGED_NOTICE: &str = "pairing-token-changed";
pub const PAIRING_TOKEN_KEYCHAIN_ID: &str = "extension-pairing-token";
const KEYCHAIN_SERVICE: &str = "com.firelink.app";

pub struct DbState {
    conn: Mutex<Connection>,
}

impl DbState {
    pub fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, String> {
        self.conn
            .lock()
            .map_err(|_| "persistence database lock is unavailable".to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PairingTokenSource {
    Keychain,
    LegacySettings,
    Generated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PairingTokenDecision {
    token: String,
    source: PairingTokenSource,
    changed: bool,
}

#[derive(Default)]
struct LegacyData {
    settings: Option<String>,
    downloads: Vec<String>,
    queues: Vec<String>,
    ownership: Vec<(String, String)>,
    pairing_token: Option<String>,
}

pub fn init(app_handle: &tauri::AppHandle) -> Result<DbState, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;
    init_at_path_internal(&app_data_dir, true)
}

#[cfg(test)]
fn init_at_path(app_data_dir: &Path) -> Result<DbState, String> {
    init_at_path_internal(app_data_dir, false)
}

fn init_at_path_internal(app_data_dir: &Path, migrate_keychain: bool) -> Result<DbState, String> {
    fs::create_dir_all(app_data_dir)
        .map_err(|error| format!("failed to create app data directory: {error}"))?;
    let database_path = app_data_dir.join(DATABASE_NAME);
    let existed = database_path.exists();
    let mut connection = Connection::open(&database_path)
        .map_err(|error| format!("failed to open database: {error}"))?;

    let version = connection
        .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
        .map_err(|error| format!("failed to read database schema version: {error}"))?;
    if existed && version < CURRENT_SCHEMA_VERSION {
        backup_database(&connection, &database_path, &format!("schema-v{version}"))?;
    }
    migrate_schema(&mut connection, version)?;
    let current_token_pending =
        sanitize_current_settings_and_restore_token(&connection, migrate_keychain)?;
    import_legacy_data(
        &mut connection,
        app_data_dir,
        migrate_keychain && !current_token_pending,
    )?;

    Ok(DbState {
        conn: Mutex::new(connection),
    })
}

fn migrate_schema(connection: &mut Connection, from_version: i64) -> Result<(), String> {
    if from_version > CURRENT_SCHEMA_VERSION {
        return Err(format!(
            "database schema version {from_version} is newer than supported version {CURRENT_SCHEMA_VERSION}"
        ));
    }

    let transaction = connection
        .transaction()
        .map_err(|error| format!("failed to begin database migration: {error}"))?;

    if from_version < 1 {
        transaction
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS settings (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    data TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS queues (
                    id TEXT PRIMARY KEY,
                    data TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS download_ownership (
                    id TEXT PRIMARY KEY,
                    primary_path TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS migration_events (
                    key TEXT PRIMARY KEY,
                    consumed INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE IF NOT EXISTS metadata (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
                ",
            )
            .map_err(|error| format!("failed to create persistence tables: {error}"))?;

        if table_exists(&transaction, "downloads")? {
            let queue_id_not_null = column_is_not_null(&transaction, "downloads", "queue_id")?;
            if queue_id_not_null {
                transaction
                    .execute_batch(
                        "
                        ALTER TABLE downloads RENAME TO downloads_v0;
                        CREATE TABLE downloads (
                            id TEXT PRIMARY KEY,
                            status TEXT NOT NULL,
                            queue_id TEXT,
                            data TEXT NOT NULL
                        );
                        INSERT INTO downloads (id, status, queue_id, data)
                            SELECT id, status, queue_id, data FROM downloads_v0;
                        DROP TABLE downloads_v0;
                        ",
                    )
                    .map_err(|error| format!("failed to migrate downloads table: {error}"))?;
            }
        } else {
            transaction
                .execute_batch(
                    "
                    CREATE TABLE downloads (
                        id TEXT PRIMARY KEY,
                        status TEXT NOT NULL,
                        queue_id TEXT,
                        data TEXT NOT NULL
                    );
                    ",
                )
                .map_err(|error| format!("failed to create downloads table: {error}"))?;
        }
    }

    transaction
        .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
        .map_err(|error| format!("failed to update database schema version: {error}"))?;
    transaction
        .commit()
        .map_err(|error| format!("failed to commit database migration: {error}"))
}

fn import_legacy_data(
    connection: &mut Connection,
    app_data_dir: &Path,
    migrate_keychain: bool,
) -> Result<(), String> {
    let legacy_app_dir = app_data_dir
        .parent()
        .map(|parent| parent.join(LEGACY_BUNDLE_IDENTIFIER));
    let candidates = [
        Some(app_data_dir.join(LEGACY_STORE_NAME)),
        legacy_app_dir.as_ref().map(|dir| dir.join(DATABASE_NAME)),
        legacy_app_dir
            .as_ref()
            .map(|dir| dir.join(LEGACY_STORE_NAME)),
    ];

    for candidate in candidates.into_iter().flatten() {
        if !candidate.exists() {
            continue;
        }
        let marker = format!("legacy-import:{}", candidate.to_string_lossy());
        if metadata_exists(connection, &marker)? {
            continue;
        }
        backup_file(&candidate, "legacy-import")?;
        let mut legacy = if candidate
            .file_name()
            .is_some_and(|name| name == DATABASE_NAME)
        {
            read_legacy_database(&candidate)?
        } else {
            read_legacy_store(&candidate)?
        };
        let mut migration_complete = true;
        if migrate_keychain
            && get_keychain_password(PAIRING_TOKEN_KEYCHAIN_ID).is_err()
            && legacy.pairing_token.is_some()
        {
            if let Some(token) = legacy.pairing_token.as_deref() {
                if let Err(error) = set_keychain_password(PAIRING_TOKEN_KEYCHAIN_ID, token) {
                    log::warn!(
                        "Legacy pairing token could not be migrated yet; settings import will retry: {}",
                        error
                    );
                    legacy.settings = None;
                    migration_complete = false;
                }
            }
        }
        merge_legacy_data(connection, legacy)?;
        if migration_complete {
            connection
                .execute(
                    "INSERT INTO metadata (key, value) VALUES (?1, 'complete')
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![marker],
                )
                .map_err(|error| format!("failed to record legacy import: {error}"))?;
        }
    }
    Ok(())
}

fn merge_legacy_data(connection: &mut Connection, legacy: LegacyData) -> Result<(), String> {
    let transaction = connection
        .transaction()
        .map_err(|error| format!("failed to begin legacy import: {error}"))?;

    let has_settings = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM settings WHERE id = 1)",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| format!("failed to inspect persisted settings: {error}"))?;
    if !has_settings {
        if let Some(settings) = legacy.settings {
            save_settings_tx(&transaction, &settings)?;
        }
    }

    let download_count: i64 = transaction
        .query_row("SELECT COUNT(*) FROM downloads", [], |row| row.get(0))
        .map_err(|error| format!("failed to inspect persisted downloads: {error}"))?;
    if download_count == 0 {
        replace_downloads_tx(&transaction, &legacy.downloads)?;
    }

    let queue_count: i64 = transaction
        .query_row("SELECT COUNT(*) FROM queues", [], |row| row.get(0))
        .map_err(|error| format!("failed to inspect persisted queues: {error}"))?;
    if queue_count == 0 {
        replace_queues_tx(&transaction, &legacy.queues)?;
    }

    for (id, primary_path) in legacy.ownership {
        transaction
            .execute(
                "INSERT OR IGNORE INTO download_ownership (id, primary_path) VALUES (?1, ?2)",
                params![id, primary_path],
            )
            .map_err(|error| format!("failed to import download ownership: {error}"))?;
    }

    transaction
        .commit()
        .map_err(|error| format!("failed to commit legacy import: {error}"))?;

    Ok(())
}

fn read_legacy_store(path: &Path) -> Result<LegacyData, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read legacy store '{}': {error}", path.display()))?;
    let document: Value = serde_json::from_str(&text).map_err(|error| {
        format!(
            "failed to decode legacy store '{}': {error}",
            path.display()
        )
    })?;

    let mut data = LegacyData::default();
    if let Some(settings) = document.get("settings") {
        let (sanitized, token) = sanitize_settings_value(settings)?;
        data.settings = Some(sanitized);
        data.pairing_token = token;
    }
    data.downloads = json_array_as_strings(document.get("download_queue"))?;
    data.queues = json_array_as_strings(document.get("queues"))?;
    data.ownership = document
        .get("download_ownership")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|record| {
            Some((
                record.get("id")?.as_str()?.to_string(),
                record.get("primaryPath")?.as_str()?.to_string(),
            ))
        })
        .collect();
    Ok(data)
}

fn read_legacy_database(path: &Path) -> Result<LegacyData, String> {
    let connection = Connection::open(path).map_err(|error| {
        format!(
            "failed to open legacy database '{}': {error}",
            path.display()
        )
    })?;
    let mut data = LegacyData::default();

    if table_exists(&connection, "settings")? {
        if let Some(settings) = connection
            .query_row("SELECT data FROM settings WHERE id = 1", [], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .map_err(|error| format!("failed to read legacy settings: {error}"))?
        {
            let (sanitized, token) = sanitize_settings_text(&settings)?;
            data.settings = Some(sanitized);
            data.pairing_token = token;
        }
    }
    if table_exists(&connection, "downloads")? {
        data.downloads = query_string_column(&connection, "SELECT data FROM downloads")?;
    }
    if table_exists(&connection, "queues")? {
        data.queues = query_string_column(&connection, "SELECT data FROM queues")?;
    }
    Ok(data)
}

fn sanitize_current_settings_and_restore_token(
    connection: &Connection,
    migrate_keychain: bool,
) -> Result<bool, String> {
    let Some(settings) = load_settings(connection)? else {
        return Ok(false);
    };
    let (sanitized, legacy_token) = sanitize_settings_text(&settings)?;
    if sanitized == settings {
        return Ok(false);
    }
    if migrate_keychain {
        if get_keychain_password(PAIRING_TOKEN_KEYCHAIN_ID).is_err() {
            if let Some(token) = legacy_token.filter(|token| !token.trim().is_empty()) {
                if let Err(error) = set_keychain_password(PAIRING_TOKEN_KEYCHAIN_ID, &token) {
                    log::warn!(
                        "Persisted pairing token could not be migrated yet; original settings retained: {}",
                        error
                    );
                    return Ok(true);
                }
            }
        }
    }
    save_settings(connection, &sanitized)?;
    Ok(false)
}

fn sanitize_settings_value(value: &Value) -> Result<(String, Option<String>), String> {
    match value {
        Value::String(text) => sanitize_settings_text(text),
        _ => sanitize_settings_document(value.clone()),
    }
}

fn sanitize_settings_text(text: &str) -> Result<(String, Option<String>), String> {
    let document: Value = serde_json::from_str(text)
        .map_err(|error| format!("failed to decode persisted settings: {error}"))?;
    sanitize_settings_document(document)
}

fn sanitize_settings_document(mut document: Value) -> Result<(String, Option<String>), String> {
    let state_value = if document.get("state").is_some() {
        document
            .get_mut("state")
            .ok_or_else(|| "persisted settings state is missing".to_string())?
    } else {
        &mut document
    };
    let state = state_value
        .as_object_mut()
        .ok_or_else(|| "persisted settings state must be an object".to_string())?;
    let token = state
        .remove("extensionPairingToken")
        .and_then(|value| value.as_str().map(str::to_string));
    let serialized = serde_json::to_string(&document)
        .map_err(|error| format!("failed to encode persisted settings: {error}"))?;
    Ok((serialized, token))
}

fn json_array_as_strings(value: Option<&Value>) -> Result<Vec<String>, String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    serde_json::to_string(item)
                        .map_err(|error| format!("failed to encode legacy item: {error}"))
                })
                .collect()
        })
        .unwrap_or_else(|| Ok(Vec::new()))
}

fn query_string_column(connection: &Connection, query: &str) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(query)
        .map_err(|error| format!("failed to prepare legacy query: {error}"))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("failed to query legacy data: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read legacy data: {error}"))
}

fn backup_file(path: &Path, reason: &str) -> Result<PathBuf, String> {
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid persistence file path '{}'", path.display()))?;
    let backup_prefix = format!("{file_name}.backup-{reason}-");
    if let Some(existing) = path.parent().and_then(|parent| {
        fs::read_dir(parent).ok()?.flatten().find_map(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(&backup_prefix)
                .then_some(entry.path())
        })
    }) {
        return Ok(existing);
    }
    let backup_path = path.with_file_name(format!("{file_name}.backup-{reason}-{timestamp}"));
    if backup_path.exists() {
        return Ok(backup_path);
    }
    fs::copy(path, &backup_path).map_err(|error| {
        format!(
            "failed to back up persistence file '{}' to '{}': {error}",
            path.display(),
            backup_path.display()
        )
    })?;
    Ok(backup_path)
}

fn backup_database(connection: &Connection, path: &Path, reason: &str) -> Result<PathBuf, String> {
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid database path '{}'", path.display()))?;
    let backup_path = path.with_file_name(format!("{file_name}.backup-{reason}-{timestamp}"));
    connection
        .execute("VACUUM INTO ?1", params![backup_path.to_string_lossy()])
        .map_err(|error| {
            format!(
                "failed to back up database '{}' to '{}': {error}",
                path.display(),
                backup_path.display()
            )
        })?;
    Ok(backup_path)
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            params![table],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to inspect database table '{table}': {error}"))
}

fn metadata_exists(connection: &Connection, key: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM metadata WHERE key = ?1)",
            params![key],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to inspect migration metadata: {error}"))
}

fn column_is_not_null(connection: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| format!("failed to inspect table '{table}': {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, bool>(3)?))
        })
        .map_err(|error| format!("failed to inspect table '{table}': {error}"))?;
    for row in rows {
        let (name, not_null) =
            row.map_err(|error| format!("failed to inspect table '{table}': {error}"))?;
        if name == column {
            return Ok(not_null);
        }
    }
    Ok(false)
}

pub fn load_settings(connection: &Connection) -> Result<Option<String>, String> {
    connection
        .query_row("SELECT data FROM settings WHERE id = 1", [], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|error| format!("failed to load settings: {error}"))
}

pub fn save_settings(connection: &Connection, data: &str) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO settings (id, data) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET data = excluded.data",
            params![data],
        )
        .map_err(|error| format!("failed to save settings: {error}"))?;
    Ok(())
}

fn save_settings_tx(transaction: &Transaction<'_>, data: &str) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO settings (id, data) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET data = excluded.data",
            params![data],
        )
        .map_err(|error| format!("failed to import settings: {error}"))?;
    Ok(())
}

pub fn load_downloads(connection: &Connection) -> Result<Vec<String>, String> {
    query_string_column(connection, "SELECT data FROM downloads ORDER BY rowid")
}

pub fn replace_downloads(connection: &mut Connection, data: &str) -> Result<(), String> {
    let values: Vec<Value> = serde_json::from_str(data)
        .map_err(|error| format!("failed to decode downloads: {error}"))?;
    let strings = values
        .iter()
        .map(|value| {
            serde_json::to_string(value)
                .map_err(|error| format!("failed to encode download: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("failed to begin download save: {error}"))?;
    replace_downloads_tx(&transaction, &strings)?;
    transaction
        .commit()
        .map_err(|error| format!("failed to commit download save: {error}"))
}

fn replace_downloads_tx(transaction: &Transaction<'_>, downloads: &[String]) -> Result<(), String> {
    transaction
        .execute("DELETE FROM downloads", [])
        .map_err(|error| format!("failed to clear downloads: {error}"))?;
    for data in downloads {
        let value: Value = serde_json::from_str(data)
            .map_err(|error| format!("failed to decode download: {error}"))?;
        let id = required_string(&value, "id")?;
        let status = required_string(&value, "status")?;
        let queue_id = value.get("queueId").and_then(Value::as_str);
        transaction
            .execute(
                "INSERT INTO downloads (id, status, queue_id, data) VALUES (?1, ?2, ?3, ?4)",
                params![id, status, queue_id, data],
            )
            .map_err(|error| format!("failed to save download '{id}': {error}"))?;
    }
    Ok(())
}

pub fn load_queues(connection: &Connection) -> Result<Vec<String>, String> {
    query_string_column(connection, "SELECT data FROM queues ORDER BY rowid")
}

pub fn replace_queues(connection: &mut Connection, data: &str) -> Result<(), String> {
    let values: Vec<Value> =
        serde_json::from_str(data).map_err(|error| format!("failed to decode queues: {error}"))?;
    let strings = values
        .iter()
        .map(|value| {
            serde_json::to_string(value).map_err(|error| format!("failed to encode queue: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("failed to begin queue save: {error}"))?;
    replace_queues_tx(&transaction, &strings)?;
    transaction
        .commit()
        .map_err(|error| format!("failed to commit queue save: {error}"))
}

fn replace_queues_tx(transaction: &Transaction<'_>, queues: &[String]) -> Result<(), String> {
    transaction
        .execute("DELETE FROM queues", [])
        .map_err(|error| format!("failed to clear queues: {error}"))?;
    for data in queues {
        let value: Value = serde_json::from_str(data)
            .map_err(|error| format!("failed to decode queue: {error}"))?;
        let id = required_string(&value, "id")?;
        transaction
            .execute(
                "INSERT INTO queues (id, data) VALUES (?1, ?2)",
                params![id, data],
            )
            .map_err(|error| format!("failed to save queue '{id}': {error}"))?;
    }
    Ok(())
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("persisted item is missing '{key}'"))
}

pub fn load_ownership(connection: &Connection) -> Result<Vec<(String, String)>, String> {
    let mut statement = connection
        .prepare("SELECT id, primary_path FROM download_ownership")
        .map_err(|error| format!("failed to prepare ownership query: {error}"))?;
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|error| format!("failed to query ownership data: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read ownership data: {error}"))
}

pub fn set_ownership(connection: &Connection, id: &str, path: &str) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO download_ownership (id, primary_path) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET primary_path = excluded.primary_path",
            params![id, path],
        )
        .map_err(|error| format!("failed to save ownership data: {error}"))?;
    Ok(())
}

pub fn remove_ownership(connection: &Connection, id: &str) -> Result<(), String> {
    connection
        .execute("DELETE FROM download_ownership WHERE id = ?1", params![id])
        .map_err(|error| format!("failed to delete ownership data: {error}"))?;
    Ok(())
}

pub fn has_user_data(connection: &Connection) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT
                EXISTS(SELECT 1 FROM settings WHERE id = 1)
                OR EXISTS(SELECT 1 FROM downloads)
                OR EXISTS(SELECT 1 FROM queues)",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to inspect existing user data: {error}"))
}

pub fn record_notice(connection: &Connection, key: &str) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO migration_events (key, consumed) VALUES (?1, 0)
             ON CONFLICT(key) DO NOTHING",
            params![key],
        )
        .map_err(|error| format!("failed to record migration notice: {error}"))?;
    Ok(())
}

pub fn has_pending_notice(connection: &Connection, key: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM migration_events WHERE key = ?1 AND consumed = 0)",
            params![key],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| format!("failed to read migration notice: {error}"))
}

pub fn consume_notice(connection: &Connection, key: &str) -> Result<(), String> {
    connection
        .execute(
            "UPDATE migration_events SET consumed = 1 WHERE key = ?1",
            params![key],
        )
        .map_err(|error| format!("failed to consume migration notice: {error}"))?;
    Ok(())
}

pub fn hydrate_pairing_token(connection: &mut Connection) -> Result<(String, bool), String> {
    let existing = get_keychain_password(PAIRING_TOKEN_KEYCHAIN_ID).ok();
    let generated = generate_pairing_token();
    let decision = decide_pairing_token(
        existing.as_deref(),
        None,
        has_user_data(connection)?,
        &generated,
    );
    if decision.source != PairingTokenSource::Keychain {
        set_keychain_password(PAIRING_TOKEN_KEYCHAIN_ID, &decision.token)?;
    }
    if decision.changed {
        record_notice(connection, TOKEN_CHANGED_NOTICE)?;
    }
    let changed = has_pending_notice(connection, TOKEN_CHANGED_NOTICE)?;
    Ok((decision.token, changed))
}

pub fn acknowledge_pairing_token_notice(connection: &Connection) -> Result<(), String> {
    consume_notice(connection, TOKEN_CHANGED_NOTICE)
}

fn decide_pairing_token(
    keychain_token: Option<&str>,
    legacy_token: Option<&str>,
    has_existing_user_data: bool,
    generated_token: &str,
) -> PairingTokenDecision {
    if let Some(token) = keychain_token.filter(|token| !token.trim().is_empty()) {
        return PairingTokenDecision {
            token: token.to_string(),
            source: PairingTokenSource::Keychain,
            changed: false,
        };
    }
    if let Some(token) = legacy_token.filter(|token| !token.trim().is_empty()) {
        return PairingTokenDecision {
            token: token.to_string(),
            source: PairingTokenSource::LegacySettings,
            changed: false,
        };
    }
    PairingTokenDecision {
        token: generated_token.to_string(),
        source: PairingTokenSource::Generated,
        changed: has_existing_user_data,
    }
}

fn generate_pairing_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

pub fn set_keychain_password(id: &str, password: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, id).map_err(|error| error.to_string())?;
    entry
        .set_password(password)
        .map_err(|error| error.to_string())
}

pub fn get_keychain_password(id: &str) -> Result<String, String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, id).map_err(|error| error.to_string())?;
    entry.get_password().map_err(|error| error.to_string())
}

pub fn delete_keychain_password(id: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, id).map_err(|error| error.to_string())?;
    let _ = entry.delete_credential();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn migrates_v0_database_and_creates_backup() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join(DATABASE_NAME);
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE downloads (
                    id TEXT PRIMARY KEY,
                    status TEXT NOT NULL,
                    queue_id TEXT NOT NULL,
                    data TEXT NOT NULL
                );
                CREATE TABLE settings (id INTEGER PRIMARY KEY, data TEXT NOT NULL);
                CREATE TABLE queues (id TEXT PRIMARY KEY, data TEXT NOT NULL);
                INSERT INTO downloads VALUES (
                    'one', 'queued', 'main',
                    '{\"id\":\"one\",\"status\":\"queued\",\"queueId\":\"main\"}'
                );
                ",
            )
            .unwrap();
        drop(connection);

        let state = init_at_path(temp.path()).unwrap();
        let connection = state.lock().unwrap();
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        assert!(!column_is_not_null(&connection, "downloads", "queue_id").unwrap());
        assert_eq!(load_downloads(&connection).unwrap().len(), 1);
        assert!(fs::read_dir(temp.path()).unwrap().flatten().any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("firelink.sqlite.backup-schema-v0-")
        }));
    }

    #[test]
    fn imports_legacy_bundle_store_and_sanitizes_token() {
        let root = TempDir::new().unwrap();
        let current = root.path().join("com.nimbold.firelink");
        let legacy = root.path().join(LEGACY_BUNDLE_IDENTIFIER);
        fs::create_dir_all(&legacy).unwrap();
        let store = json!({
            "settings": json!({
                "state": {
                    "theme": "dark",
                    "extensionPairingToken": "legacy-secret"
                },
                "version": 0
            }).to_string(),
            "download_queue": [{
                "id": "download-1",
                "status": "ready",
                "url": "https://example.com/file",
                "fileName": "file",
                "category": "Other",
                "dateAdded": ""
            }],
            "queues": [{
                "id": "main",
                "name": "Main Queue",
                "isMain": true
            }]
        });
        fs::write(
            legacy.join(LEGACY_STORE_NAME),
            serde_json::to_vec(&store).unwrap(),
        )
        .unwrap();

        let state = init_at_path(&current).unwrap();
        let connection = state.lock().unwrap();
        assert_eq!(load_downloads(&connection).unwrap().len(), 1);
        assert_eq!(load_queues(&connection).unwrap().len(), 1);
        let settings = load_settings(&connection).unwrap().unwrap();
        assert!(settings.contains("\"theme\":\"dark\""));
        assert!(!settings.contains("legacy-secret"));
        assert!(fs::read_dir(&legacy).unwrap().flatten().any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("store.bin.backup-legacy-import-")
        }));
    }

    #[test]
    fn imports_legacy_bundle_sqlite_database() {
        let root = TempDir::new().unwrap();
        let current = root.path().join("com.nimbold.firelink");
        let legacy = root.path().join(LEGACY_BUNDLE_IDENTIFIER);
        fs::create_dir_all(&legacy).unwrap();
        let legacy_path = legacy.join(DATABASE_NAME);
        let connection = Connection::open(&legacy_path).unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE downloads (
                    id TEXT PRIMARY KEY,
                    status TEXT NOT NULL,
                    queue_id TEXT NOT NULL,
                    data TEXT NOT NULL
                );
                CREATE TABLE settings (id INTEGER PRIMARY KEY, data TEXT NOT NULL);
                CREATE TABLE queues (id TEXT PRIMARY KEY, data TEXT NOT NULL);
                INSERT INTO downloads VALUES (
                    'legacy-download', 'queued', 'legacy-main',
                    '{\"id\":\"legacy-download\",\"status\":\"queued\",\"queueId\":\"legacy-main\"}'
                );
                INSERT INTO queues VALUES (
                    'legacy-main',
                    '{\"id\":\"legacy-main\",\"name\":\"Legacy Main\",\"isMain\":true}'
                );
                INSERT INTO settings VALUES (
                    1,
                    '{\"state\":{\"theme\":\"nord\"},\"version\":0}'
                );
                ",
            )
            .unwrap();
        drop(connection);

        let state = init_at_path(&current).unwrap();
        let connection = state.lock().unwrap();
        assert_eq!(load_downloads(&connection).unwrap().len(), 1);
        assert_eq!(load_queues(&connection).unwrap().len(), 1);
        assert!(load_settings(&connection)
            .unwrap()
            .unwrap()
            .contains("\"nord\""));
        assert!(fs::read_dir(&legacy).unwrap().flatten().any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("firelink.sqlite.backup-legacy-import-")
        }));
    }

    #[test]
    fn token_decision_preserves_keychain_and_legacy_values() {
        let keychain = decide_pairing_token(Some("keychain"), Some("legacy"), true, "generated");
        assert_eq!(keychain.token, "keychain");
        assert_eq!(keychain.source, PairingTokenSource::Keychain);
        assert!(!keychain.changed);

        let legacy = decide_pairing_token(None, Some("legacy"), true, "generated");
        assert_eq!(legacy.token, "legacy");
        assert_eq!(legacy.source, PairingTokenSource::LegacySettings);
        assert!(!legacy.changed);

        let recovery = decide_pairing_token(None, None, true, "generated");
        assert_eq!(recovery.token, "generated");
        assert_eq!(recovery.source, PairingTokenSource::Generated);
        assert!(recovery.changed);

        let fresh = decide_pairing_token(None, None, false, "generated");
        assert!(!fresh.changed);
    }

    #[test]
    fn migration_notice_is_persistent_until_acknowledged() {
        let temp = TempDir::new().unwrap();
        let state = init_at_path(temp.path()).unwrap();
        let connection = state.lock().unwrap();

        record_notice(&connection, TOKEN_CHANGED_NOTICE).unwrap();
        assert!(has_pending_notice(&connection, TOKEN_CHANGED_NOTICE).unwrap());
        assert!(has_pending_notice(&connection, TOKEN_CHANGED_NOTICE).unwrap());

        acknowledge_pairing_token_notice(&connection).unwrap();
        assert!(!has_pending_notice(&connection, TOKEN_CHANGED_NOTICE).unwrap());
    }
}
