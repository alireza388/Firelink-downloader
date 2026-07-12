use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const DATABASE_NAME: &str = "firelink.sqlite";
const LEGACY_STORE_NAME: &str = "store.bin";
const LEGACY_BUNDLE_IDENTIFIER: &str = "com.nima.tauri-app";
const CURRENT_SCHEMA_VERSION: i64 = 1;
const TOKEN_CHANGED_NOTICE: &str = "pairing-token-changed";
pub const PAIRING_TOKEN_KEYCHAIN_ID: &str = "extension-pairing-token";
const KEYCHAIN_SERVICE: &str = "com.firelink.app";
static KEYRING_OPERATION_LOCK: Mutex<()> = Mutex::new(());

pub struct DbState {
    conn: Mutex<Connection>,
    portable: bool,
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

pub fn init(storage_layout: &crate::storage::StorageLayout) -> Result<DbState, String> {
    init_at_path_internal(storage_layout.data_dir(), storage_layout.is_portable())
}

#[cfg(test)]
fn init_at_path(app_data_dir: &Path) -> Result<DbState, String> {
    init_at_path_internal(app_data_dir, false)
}

fn init_at_path_internal(app_data_dir: &Path, portable: bool) -> Result<DbState, String> {
    fs::create_dir_all(app_data_dir)
        .map_err(|error| format!("failed to create app data directory: {error}"))?;
    let database_path = app_data_dir.join(DATABASE_NAME);
    let existed = database_path.exists();
    let mut connection = Connection::open(&database_path)
        .map_err(|error| format!("failed to open database: {error}"))?;

    let version = connection
        .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
        .map_err(|error| format!("failed to read database schema version: {error}"))?;
    // Portable mode intentionally does not create raw migration backups:
    // those backups would duplicate any legacy transfer secrets beside the
    // executable. The imported data is sanitized before the portable DB is
    // used, and any legacy source is sanitized in place after a successful
    // import so it cannot remain as an unsanitized sidecar.
    if existed && version < CURRENT_SCHEMA_VERSION && !portable {
        backup_database(&connection, &database_path, &format!("schema-v{version}"))?;
    }
    migrate_schema(&mut connection, version)?;

    // We no longer touch the keychain on backend startup.
    // Legacy imports will safely preserve any pairing token in the JSON payload.
    // The frontend will manually trigger migration to the keychain via IPC if access is granted.
    import_legacy_data(&mut connection, app_data_dir, false, portable)?;
    if portable {
        sanitize_persisted_downloads(&mut connection)?;
    }

    Ok(DbState {
        conn: Mutex::new(connection),
        portable,
    })
}

impl DbState {
    pub fn is_portable(&self) -> bool {
        self.portable
    }
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
                        DROP TABLE IF EXISTS downloads_v0;
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
    portable: bool,
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
            if portable {
                sanitize_legacy_source(&candidate)?;
            }
            continue;
        }
        if !portable {
            backup_file(&candidate, "legacy-import")?;
        }
        let mut legacy = if candidate
            .file_name()
            .is_some_and(|name| name == DATABASE_NAME)
        {
            read_legacy_database(&candidate, migrate_keychain)?
        } else {
            read_legacy_store(&candidate, migrate_keychain)?
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
            if portable {
                sanitize_legacy_source(&candidate)?;
            }
        }
    }
    Ok(())
}

fn sanitize_legacy_source(path: &Path) -> Result<(), String> {
    if path
        .file_name()
        .is_some_and(|name| name == DATABASE_NAME)
    {
        let mut connection = Connection::open(path).map_err(|error| {
            format!(
                "failed to open legacy database '{}' for portable sanitization: {error}",
                path.display()
            )
        })?;
        if table_exists(&connection, "downloads")? {
            return sanitize_persisted_downloads(&mut connection);
        }
        return Ok(());
    }

    let text = fs::read_to_string(path).map_err(|error| {
        format!(
            "failed to read legacy store '{}' for portable sanitization: {error}",
            path.display()
        )
    })?;
    let mut document: Value = serde_json::from_str(&text).map_err(|error| {
        format!(
            "failed to decode legacy store '{}' for portable sanitization: {error}",
            path.display()
        )
    })?;
    let Some(downloads) = document
        .get_mut("download_queue")
        .and_then(Value::as_array_mut)
    else {
        return Ok(());
    };
    for download in downloads {
        remove_persisted_transfer_secrets(download);
    }
    let sanitized = serde_json::to_string(&document).map_err(|error| {
        format!(
            "failed to encode legacy store '{}' for portable sanitization: {error}",
            path.display()
        )
    })?;
    if sanitized == text {
        return Ok(());
    }

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid legacy store path '{}'", path.display()))?;
    let temporary = path.with_file_name(format!(".{file_name}.portable-sanitized.tmp"));
    fs::write(&temporary, sanitized).map_err(|error| {
        format!(
            "failed to write temporary sanitized legacy store '{}': {error}",
            temporary.display()
        )
    })?;
    if let Err(rename_error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(path);
        fs::rename(&temporary, path).map_err(|error| {
            format!(
                "failed to replace legacy store '{}' after rename error ({rename_error}): {error}",
                path.display()
            )
        })?;
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

fn read_legacy_store(path: &Path, force_migrate: bool) -> Result<LegacyData, String> {
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
        let (sanitized, token, _) = sanitize_settings_value(settings, force_migrate)?;
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

fn read_legacy_database(path: &Path, force_migrate: bool) -> Result<LegacyData, String> {
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
            let (sanitized, token, _) = sanitize_settings_text(&settings, force_migrate)?;
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

pub fn sanitize_current_settings_and_restore_token(
    connection: &Connection,
    force_migrate: bool,
) -> Result<(bool, bool), String> {
    let Some(settings) = load_settings(connection)? else {
        return Ok((false, false));
    };
    let (sanitized, legacy_token, keychain_granted) =
        sanitize_settings_text(&settings, force_migrate)?;
    if sanitized == settings {
        return Ok((false, keychain_granted));
    }
    let should_migrate = force_migrate || keychain_granted;
    if should_migrate && get_keychain_password(PAIRING_TOKEN_KEYCHAIN_ID).is_err() {
        if let Some(token) = legacy_token.filter(|token| !token.trim().is_empty()) {
            if let Err(error) = set_keychain_password(PAIRING_TOKEN_KEYCHAIN_ID, &token) {
                log::warn!(
                        "Persisted pairing token could not be migrated yet; original settings retained: {}",
                        error
                    );
                return Ok((true, keychain_granted));
            }
        }
    }
    save_settings(connection, &sanitized)?;
    Ok((false, keychain_granted))
}

fn sanitize_settings_value(
    value: &Value,
    force_migrate: bool,
) -> Result<(String, Option<String>, bool), String> {
    match value {
        Value::String(text) => sanitize_settings_text(text, force_migrate),
        _ => sanitize_settings_document(value.clone(), force_migrate),
    }
}

fn sanitize_settings_text(
    text: &str,
    force_migrate: bool,
) -> Result<(String, Option<String>, bool), String> {
    let document: Value = serde_json::from_str(text)
        .map_err(|error| format!("failed to decode persisted settings: {error}"))?;
    sanitize_settings_document(document, force_migrate)
}

fn sanitize_settings_document(
    mut document: Value,
    force_migrate: bool,
) -> Result<(String, Option<String>, bool), String> {
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

    let keychain_granted = state
        .get("keychainAccessGranted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let should_migrate = force_migrate || keychain_granted;

    let token = if should_migrate {
        state
            .remove("extensionPairingToken")
            .and_then(|value| value.as_str().map(str::to_string))
    } else {
        None
    };

    let serialized = serde_json::to_string(&document)
        .map_err(|error| format!("failed to encode persisted settings: {error}"))?;
    Ok((serialized, token, keychain_granted))
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

#[allow(dead_code)]
pub fn is_keychain_access_granted(connection: &Connection) -> Result<bool, String> {
    let Some(settings) = load_settings(connection)? else {
        return Ok(false);
    };
    let document: Value = serde_json::from_str(&settings)
        .map_err(|error| format!("failed to decode settings: {error}"))?;
    let granted = document
        .get("state")
        .and_then(|s| s.get("keychainAccessGranted"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Ok(granted)
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

pub fn replace_downloads(
    connection: &mut Connection,
    data: &str,
    portable: bool,
) -> Result<(), String> {
    let values: Vec<Value> = serde_json::from_str(data)
        .map_err(|error| format!("failed to decode downloads: {error}"))?;
    let strings = values
        .into_iter()
        .map(|mut value| {
            if portable {
                remove_persisted_transfer_secrets(&mut value);
            }
            serde_json::to_string(&value)
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

fn remove_persisted_transfer_secrets(value: &mut Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };

    // These values are accepted from users, browser extensions, or URLs and
    // may contain credentials or bearer tokens. Portable queues keep their
    // useful metadata, but never persist these values beside the executable.
    for key in ["password", "cookies", "headers", "mirrors", "proxy"] {
        object.remove(key);
    }

    if let Some(url) = object.get("url").and_then(Value::as_str) {
        if let Ok(mut parsed) = url::Url::parse(url) {
            let had_userinfo = !parsed.username().is_empty() || parsed.password().is_some();
            let had_query_or_fragment = parsed.query().is_some() || parsed.fragment().is_some();
            if had_userinfo || had_query_or_fragment {
                let _ = parsed.set_username("");
                let _ = parsed.set_password(None);
                parsed.set_query(None);
                parsed.set_fragment(None);
                object.insert("url".to_string(), Value::String(parsed.to_string()));

                // A queued transfer whose URL depended on query/fragment
                // credentials must not silently auto-resume with a truncated
                // URL after a portable restart.
                if had_userinfo || had_query_or_fragment {
                    mark_portable_download_unresumable(object);
                }
            }
        } else {
            object.insert("url".to_string(), Value::String(String::new()));
            mark_portable_download_unresumable(object);
        }
    }
}

fn mark_portable_download_unresumable(object: &mut serde_json::Map<String, Value>) {
    if object
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| status != "completed")
    {
        object.insert("status".to_string(), Value::String("failed".to_string()));
        object.insert("resumable".to_string(), Value::Bool(false));
        object.insert(
            "lastError".to_string(),
            Value::String(
                "Portable mode removed credentials from this persisted download; add it again to resume."
                    .to_string(),
            ),
        );
    }
}

fn sanitize_persisted_downloads(connection: &mut Connection) -> Result<(), String> {
    let transaction = connection
        .transaction()
        .map_err(|error| format!("failed to begin portable download sanitization: {error}"))?;
    let records = {
        let mut statement = transaction
            .prepare("SELECT id, data FROM downloads")
            .map_err(|error| {
                format!("failed to prepare portable download sanitization: {error}")
            })?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| {
                format!("failed to read downloads for portable sanitization: {error}")
            })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
            format!("failed to read download for portable sanitization: {error}")
        })?
    };

    for (id, data) in records {
        let mut value: Value = serde_json::from_str(&data).map_err(|error| {
            format!("failed to decode download '{id}' for portable sanitization: {error}")
        })?;
        remove_persisted_transfer_secrets(&mut value);
        let sanitized = serde_json::to_string(&value).map_err(|error| {
            format!("failed to encode download '{id}' for portable sanitization: {error}")
        })?;
        if sanitized != data {
            transaction
                .execute(
                    "UPDATE downloads SET data = ?1 WHERE id = ?2",
                    params![sanitized, id],
                )
                .map_err(|error| format!("failed to sanitize download '{id}': {error}"))?;
        }
    }

    transaction
        .commit()
        .map_err(|error| format!("failed to commit portable download sanitization: {error}"))
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

pub fn hydrate_pairing_token(
    connection: &mut Connection,
    skip_keychain: bool,
) -> Result<(String, bool), String> {
    if skip_keychain {
        return Ok((generate_pairing_token(), false));
    }

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

pub(crate) fn generate_pairing_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// Read the extension pairing token from the persisted settings JSON.
/// Returns `None` when the field is missing, empty, or the settings haven't
/// been saved yet.  This is the **primary read path** — it does not touch the
/// OS keychain and therefore never triggers a system credential prompt.
pub fn load_pairing_token_from_settings(connection: &Connection) -> Result<Option<String>, String> {
    let Some(settings_json) = load_settings(connection)? else {
        return Ok(None);
    };
    let value: serde_json::Value = serde_json::from_str(&settings_json)
        .map_err(|error| format!("failed to decode settings: {error}"))?;
    let state = value.get("state").unwrap_or(&value);
    let token = state
        .get("extensionPairingToken")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    Ok(token)
}

/// Write (or update) the extension pairing token inside the persisted settings
/// JSON document.  Keeps all other settings fields intact.
pub fn save_pairing_token_to_settings(
    connection: &Connection,
    token: &str,
    initialize_if_missing: bool,
) -> Result<(), String> {
    let Some(settings_json) = load_settings(connection)? else {
        if !initialize_if_missing {
            // Settings have not been persisted yet. Standard mode keeps the
            // first-run token session-only until the user grants credential
            // store access; portable mode opts into initialization explicitly.
            return Ok(());
        }
        let initial = serde_json::json!({
            "state": { "extensionPairingToken": token },
            "version": 3
        });
        let serialized = serde_json::to_string(&initial)
            .map_err(|error| format!("failed to encode initial settings: {error}"))?;
        return save_settings(connection, &serialized);
    };
    let mut value: serde_json::Value = serde_json::from_str(&settings_json)
        .map_err(|error| format!("failed to decode settings: {error}"))?;
    let state = if value.get("state").is_some() {
        value
            .get_mut("state")
            .expect("state is an object")
    } else {
        &mut value
    };
    state["extensionPairingToken"] =
        serde_json::Value::String(token.to_string());
    let updated = serde_json::to_string(&value)
        .map_err(|error| format!("failed to encode settings: {error}"))?;
    save_settings(connection, &updated)
}

fn ensure_keyring_store() -> Result<(), String> {
    if keyring_core::get_default_store().is_some() {
        return Ok(());
    }

    static STORE_INIT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = STORE_INIT_LOCK
        .lock()
        .map_err(|_| "keyring store initialization lock is unavailable".to_string())?;

    if keyring_core::get_default_store().is_some() {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let store = apple_native_keyring_store::keychain::Store::new()
            .map_err(|error| error.to_string())?;
        keyring_core::set_default_store(store);
        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        let store =
            windows_native_keyring_store::Store::new().map_err(|error| error.to_string())?;
        keyring_core::set_default_store(store);
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        let store =
            zbus_secret_service_keyring_store::Store::new().map_err(|error| error.to_string())?;
        keyring_core::set_default_store(store);
        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err("No native keyring store is available for this platform".to_string())
    }
}

fn keychain_entry_with_target(
    id: &str,
    target: Option<&str>,
) -> Result<keyring_core::Entry, String> {
    ensure_keyring_store()?;
    if let Some(target) = target {
        return keyring_core::Entry::new_with_modifiers(
            KEYCHAIN_SERVICE,
            id,
            &std::collections::HashMap::from([("target", target)]),
        )
        .map_err(|error| error.to_string());
    }
    keyring_core::Entry::new(KEYCHAIN_SERVICE, id).map_err(|error| error.to_string())
}

fn keychain_entry(id: &str) -> Result<keyring_core::Entry, String> {
    #[cfg(target_os = "linux")]
    {
        return keychain_entry_with_target(id, Some("default"));
    }

    #[cfg(not(target_os = "linux"))]
    keychain_entry_with_target(id, None)
}

fn lock_keyring_operations() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    KEYRING_OPERATION_LOCK
        .lock()
        .map_err(|_| "keyring operation lock is unavailable".to_string())
}

#[cfg(target_os = "linux")]
fn legacy_linux_keychain_entries(id: &str) -> Result<Vec<keyring_core::Entry>, String> {
    ensure_keyring_store()?;
    let entries = keyring_core::Entry::search(&std::collections::HashMap::from([
        ("service", KEYCHAIN_SERVICE),
        ("username", id),
    ]))
    .map_err(|error| error.to_string())?;
    let mut legacy = Vec::new();
    for entry in entries {
        let attributes = entry.get_attributes().map_err(|error| error.to_string())?;
        if !attributes.contains_key("target") {
            legacy.push(entry);
        }
    }
    Ok(legacy)
}

#[cfg(target_os = "linux")]
fn unique_legacy_linux_keychain_entry(id: &str) -> Result<Option<keyring_core::Entry>, String> {
    let mut entries = legacy_linux_keychain_entries(id)?;
    match entries.len() {
        0 => Ok(None),
        1 => Ok(entries.pop()),
        count => Err(format!(
            "Entry is matched by {count} legacy Linux credentials"
        )),
    }
}

pub fn set_keychain_password(id: &str, password: &str) -> Result<(), String> {
    let _guard = lock_keyring_operations()?;
    let entry = keychain_entry(id)?;

    #[cfg(target_os = "linux")]
    if let Err(error) = entry.get_credential() {
        match error {
            keyring_core::Error::NoEntry => {
                if let Some(legacy) = unique_legacy_linux_keychain_entry(id)? {
                    return legacy
                        .set_password(password)
                        .map_err(|error| error.to_string());
                }
            }
            error => return Err(error.to_string()),
        }
    }

    entry
        .set_password(password)
        .map_err(|error| error.to_string())
}

pub fn get_keychain_password(id: &str) -> Result<String, String> {
    let _guard = lock_keyring_operations()?;
    let entry = keychain_entry(id)?;
    match entry.get_password() {
        Ok(password) => Ok(password),
        #[cfg(target_os = "linux")]
        Err(keyring_core::Error::NoEntry) => unique_legacy_linux_keychain_entry(id)?
            .ok_or_else(|| keyring_core::Error::NoEntry.to_string())?
            .get_password()
            .map_err(|error| error.to_string()),
        #[cfg(target_os = "linux")]
        Err(error) => Err(error.to_string()),
        #[cfg(not(target_os = "linux"))]
        Err(error) => Err(error.to_string()),
    }
}

pub fn delete_keychain_password(id: &str) -> Result<(), String> {
    let _guard = lock_keyring_operations()?;
    let entry = keychain_entry(id)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => {}
        Err(error) => return Err(error.to_string()),
    }

    #[cfg(target_os = "linux")]
    for legacy in legacy_linux_keychain_entries(id)? {
        match legacy.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => {}
            Err(error) => return Err(error.to_string()),
        }
    }

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
    fn portable_migration_does_not_create_raw_schema_backup() {
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
                    '{\"id\":\"one\",\"status\":\"queued\",\"password\":\"secret\"}'
                );
                ",
            )
            .unwrap();
        drop(connection);

        let state = init_at_path_internal(temp.path(), true).unwrap();
        let connection = state.lock().unwrap();
        let saved: Value = serde_json::from_str(&load_downloads(&connection).unwrap()[0]).unwrap();
        assert!(saved.get("password").is_none());
        assert!(!fs::read_dir(temp.path()).unwrap().flatten().any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("firelink.sqlite.backup-schema-v0-")
        }));
    }

    #[test]
    fn imports_legacy_bundle_store_and_preserves_token() {
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
        assert!(settings.contains("legacy-secret"));
        assert!(fs::read_dir(&legacy).unwrap().flatten().any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("store.bin.backup-legacy-import-")
        }));
    }

    #[test]
    fn portable_import_sanitizes_legacy_source_after_success() {
        let root = TempDir::new().unwrap();
        let current = root.path().join("com.nimbold.firelink");
        let legacy = root.path().join(LEGACY_BUNDLE_IDENTIFIER);
        fs::create_dir_all(&legacy).unwrap();
        let store_path = legacy.join(LEGACY_STORE_NAME);
        let store = json!({
            "settings": json!({"state": {"theme": "dark"}}).to_string(),
            "download_queue": [{
                "id": "download-1",
                "status": "queued",
                "url": "https://example.com/file",
                "password": "legacy-secret"
            }],
            "queues": []
        });
        fs::write(&store_path, serde_json::to_vec(&store).unwrap()).unwrap();

        let state = init_at_path_internal(&current, true).unwrap();
        let connection = state.lock().unwrap();
        let saved: Value = serde_json::from_str(&load_downloads(&connection).unwrap()[0]).unwrap();
        assert!(saved.get("password").is_none());
        let sanitized_store = fs::read_to_string(&store_path).unwrap();
        assert!(!sanitized_store.contains("legacy-secret"));
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
    fn portable_download_persistence_removes_transfer_secrets() {
        let temp = TempDir::new().unwrap();
        let state = init_at_path(temp.path()).unwrap();
        let mut connection = state.lock().unwrap();
        let data = json!([{
            "id": "download-1",
            "status": "queued",
            "queueId": "main",
            "url": "https://user:secret@example.com/file?token=secret#fragment",
            "password": "secret",
            "cookies": "session=secret",
            "headers": "Authorization: Bearer secret",
            "mirrors": "https://user:secret@example.com/mirror",
            "proxy": "http://user:secret@example.com:8080"
        }])
        .to_string();

        replace_downloads(&mut connection, &data, true).unwrap();

        let saved: Value = serde_json::from_str(&load_downloads(&connection).unwrap()[0]).unwrap();
        assert_eq!(saved["url"], "https://example.com/file");
        assert_eq!(saved["status"], "failed");
        assert_eq!(saved["resumable"], false);
        assert!(!saved.to_string().contains("secret"));
        for key in ["password", "cookies", "headers", "mirrors", "proxy"] {
            assert!(saved.get(key).is_none(), "portable data retained {key}");
        }
    }

    #[test]
    fn portable_persistence_redacts_unparseable_download_urls() {
        let temp = TempDir::new().unwrap();
        let state = init_at_path(temp.path()).unwrap();
        let mut connection = state.lock().unwrap();
        let data = json!([{
            "id": "download-1",
            "status": "queued",
            "url": "not a URL secret=secret"
        }])
        .to_string();

        replace_downloads(&mut connection, &data, true).unwrap();

        let saved: Value = serde_json::from_str(&load_downloads(&connection).unwrap()[0]).unwrap();
        assert_eq!(saved["url"], "");
        assert_eq!(saved["status"], "failed");
        assert!(!saved.to_string().contains("secret"));
    }

    #[test]
    fn portable_initialization_sanitizes_existing_downloads() {
        let temp = TempDir::new().unwrap();
        let state = init_at_path(temp.path()).unwrap();
        let mut connection = state.lock().unwrap();
        let data = json!([{
            "id": "download-1",
            "status": "queued",
            "url": "https://example.com/file",
            "password": "secret"
        }])
        .to_string();
        replace_downloads(&mut connection, &data, false).unwrap();
        drop(connection);
        drop(state);

        let state = init_at_path_internal(temp.path(), true).unwrap();
        let connection = state.lock().unwrap();
        let saved: Value = serde_json::from_str(&load_downloads(&connection).unwrap()[0]).unwrap();
        assert!(saved.get("password").is_none());
    }

    #[test]
    fn pairing_token_is_persisted_before_frontend_settings_exist() {
        let temp = TempDir::new().unwrap();
        let state = init_at_path(temp.path()).unwrap();
        let connection = state.lock().unwrap();

        save_pairing_token_to_settings(&connection, "initial-token", true).unwrap();

        assert_eq!(
            load_pairing_token_from_settings(&connection).unwrap().as_deref(),
            Some("initial-token")
        );
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
