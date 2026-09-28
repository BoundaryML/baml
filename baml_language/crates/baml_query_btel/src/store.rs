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

/// Page cache between refreshes, in KiB (negative: SQLite's KiB unit).
pub(crate) const QUERY_CACHE_KIB: i64 = -65_536;
/// Page cache while applying files: an index transaction touches pages all
/// over large tables, and a first index writes all of them before commit.
pub(crate) const REFRESH_CACHE_KIB: i64 = -131_072;
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
    conn.pragma_update(None, "cache_size", QUERY_CACHE_KIB)?;
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
    write_statistics(&tx)?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(true)
}

/// Rows the planner assumes per table: fixed, so plans do not depend on
/// what is indexed so far.
const PLANNED_ROWS: u64 = 1_000_000;

/// Planner statistics for the index's shape rather than its contents: a
/// recording (`rec`) matches many rows, a whole key one, and a non-unique
/// id (a thread's calls, an execution's threads) about ten. Without statistics
/// SQLite assumes `rec` alone is selective and reads a recording's rows by
/// it instead of using an index on the id that was asked for.
pub(crate) fn write_statistics(tx: &rusqlite::Transaction<'_>) -> Result<(), Error> {
    tx.execute_batch("ANALYZE sqlite_schema; DELETE FROM sqlite_stat1")?;
    let tables: Vec<String> = tx
        .prepare(
            "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for table in tables {
        let indexes: Vec<(String, bool, bool)> = tx
            .prepare("SELECT name, \"unique\", partial FROM pragma_index_list(?1)")?
            .query_map([&table], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<_, _>>()?;
        for (index, unique, partial) in indexes {
            let columns: Vec<Option<String>> = tx
                .prepare("SELECT name FROM pragma_index_xinfo(?1) WHERE key = 1 ORDER BY seqno")?
                .query_map([&index], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            let rows = if partial {
                PLANNED_ROWS / 1_000
            } else {
                PLANNED_ROWS
            };
            let mut stat = vec![rows];
            for (i, column) in columns.iter().enumerate() {
                let previous = *stat.last().expect("row count");
                stat.push(if unique && i + 1 == columns.len() {
                    1
                } else if i == 0 && column.as_deref() == Some("rec") {
                    rows / 10
                } else {
                    (previous / 10).clamp(1, 10)
                });
            }
            let stat: Vec<String> = stat.iter().map(u64::to_string).collect();
            tx.execute(
                "INSERT INTO sqlite_stat1 (tbl, idx, stat) VALUES (?1, ?2, ?3)",
                rusqlite::params![table, index, stat.join(" ")],
            )?;
        }
    }
    tx.execute_batch("ANALYZE sqlite_schema")?;
    Ok(())
}
