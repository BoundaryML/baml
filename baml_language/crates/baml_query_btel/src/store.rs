//! The derived database file and its writer lock.
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use btel_reader::layout::SourceLayout;
use fs2::FileExt;
use rusqlite::{Connection, OpenFlags, OptionalExtension as _};

use crate::{
    Error,
    schema::{ALL_TABLES, DDL, NORMALIZATION_VERSION, SCHEMA_VERSION},
};

pub const DATABASE_FILE: &str = "query.sqlite";
pub const LOCK_FILE: &str = "query.lock";

/// Serializes database creation, rebuilds and reconciliation across
/// processes. Readers never take it. Released on drop.
pub struct WriterLock {
    file: File,
}

impl WriterLock {
    /// Wait up to `timeout` for ownership. Timing out is an error, never a
    /// silent fallback to possibly stale results.
    pub fn acquire(path: &Path, timeout: Duration) -> Result<(Self, Duration), Error> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| Error::Io(format!("{}: {e}", path.display())))?;
        let start = Instant::now();
        let mut pause = Duration::from_millis(2);
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => return Ok((Self { file }, start.elapsed())),
                Err(error)
                    if error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
                        || error.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    if start.elapsed() >= timeout {
                        return Err(Error::LockTimeout(timeout));
                    }
                    std::thread::sleep(pause.min(timeout.saturating_sub(start.elapsed())));
                    pause = (pause * 2).min(Duration::from_millis(50));
                }
                Err(error) => return Err(Error::Io(format!("{}: {error}", path.display()))),
            }
        }
    }
}

impl Drop for WriterLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

#[derive(Clone, Debug)]
pub struct StoreOptions {
    /// SQLite busy timeout for statements that meet another writer.
    pub busy_timeout: Duration,
    /// Pages between automatic WAL checkpoints.
    pub wal_autocheckpoint: u32,
}
impl Default for StoreOptions {
    fn default() -> Self {
        Self {
            busy_timeout: Duration::from_secs(5),
            wal_autocheckpoint: 1000,
        }
    }
}

pub fn database_path(layout: &SourceLayout) -> PathBuf {
    layout.root.join(DATABASE_FILE)
}

pub fn lock_path(layout: &SourceLayout) -> PathBuf {
    layout.root.join(LOCK_FILE)
}

/// Open (creating the file if needed) with WAL and NORMAL sync. Switching a
/// new file to WAL needs an exclusive lock that SQLite does not wait for, so
/// it happens under the writer lock; tables are created later, also under
/// that lock, in [`ensure_schema`].
pub fn open(
    path: &Path,
    options: &StoreOptions,
    lock: &Path,
    lock_timeout: Duration,
) -> Result<Connection, Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::Io(format!("{}: {e}", parent.display())))?;
    }
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(options.busy_timeout)?;
    let mode: String = conn.pragma_query_value(None, "journal_mode", |r| r.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        let _owner = WriterLock::acquire(lock, lock_timeout)?;
        conn.pragma_update(None, "journal_mode", "wal")?;
    }
    conn.pragma_update(None, "synchronous", "normal")?;
    conn.pragma_update(None, "temp_store", "memory")?;
    conn.pragma_update(None, "wal_autocheckpoint", options.wal_autocheckpoint)?;
    Ok(conn)
}

/// Whether the database carries this binary's schema and normalization.
pub fn versions_match(conn: &Connection) -> Result<bool, Error> {
    let user_version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if user_version != SCHEMA_VERSION {
        return Ok(false);
    }
    let normalization: Option<i64> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'normalization_version'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(normalization == Some(NORMALIZATION_VERSION))
}

/// Create or rebuild the schema. Call with the writer lock held. The rebuild
/// is one transaction: readers see either the old or the new database.
/// Returns whether a (re)build happened.
pub fn ensure_schema(conn: &mut Connection) -> Result<bool, Error> {
    if versions_match(conn).unwrap_or(false) {
        return Ok(false);
    }
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let existing: Vec<(String, String)> = tx
        .prepare("SELECT type, name FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (kind, name) in existing {
        if kind == "table" || kind == "view" {
            let kind = if kind == "table" { "TABLE" } else { "VIEW" };
            tx.execute_batch(&format!(
                "DROP {kind} IF EXISTS \"{}\"",
                name.replace('"', "\"\"")
            ))?;
        }
    }
    debug_assert!(ALL_TABLES.contains(&"meta"));
    tx.execute_batch(DDL)?;
    tx.execute(
        "INSERT INTO meta (key, value) VALUES ('normalization_version', ?1)",
        [NORMALIZATION_VERSION],
    )?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(true)
}
