use std::{
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use baml_builtins2::{SqlArrayType, SqlBindValue, SqlStatement};
use baml_type::RuntimeTy;
use bex_external_types::{AsBexExternalValue, BexExternalValue};
use bex_heap::BexHeap;
use bigdecimal::BigDecimal;
use futures::TryStreamExt;
use indexmap::IndexMap;
use num_bigint::{BigInt, Sign, ToBigInt};
use num_traits::ToPrimitive;
use sqlx::{
    Arguments, Column, Decode, Executor, Pool, Row, TypeInfo, ValueRef,
    pool::PoolConnection,
    postgres::{
        PgArguments, PgConnectOptions, PgPoolOptions, PgRow, PgSslMode, PgValueRef, Postgres,
        types::PgInterval,
    },
    sqlite::{
        Sqlite, SqliteArguments, SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions,
        SqliteRow, SqliteValueRef,
    },
    types::Json,
};
use sys_ops::io::{self, CallId, SysOpContext, SysOpOutput, owned};
use sys_types::OpErrorBody;
use time::{
    Date, Duration as TimeDuration, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset,
    format_description::{
        BorrowedFormatItem,
        well_known::{Iso8601, Rfc3339},
    },
    macros::format_description,
};
use tokio::sync::Mutex;

use crate::NativeSysOps;

const PLAIN_DATETIME_FORMAT: &[BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second][optional [.[subsecond]]]");
const PLAIN_TIME_FORMAT: &[BorrowedFormatItem<'_>] =
    format_description!("[hour]:[minute]:[second][optional [.[subsecond]]]");

enum DatabasePool {
    Postgres(Pool<Postgres>),
    Sqlite(Pool<Sqlite>),
}

struct DatabaseHandle {
    pool: DatabasePool,
    default_timeout: Option<Duration>,
    closed: AtomicBool,
}

enum TransactionConnection {
    Postgres(PoolConnection<Postgres>),
    Sqlite(PoolConnection<Sqlite>),
}

impl TransactionConnection {
    fn close_on_drop(&mut self) {
        match self {
            Self::Postgres(connection) => connection.close_on_drop(),
            Self::Sqlite(connection) => connection.close_on_drop(),
        }
    }
}

/// A provider operation can be dropped when its BAML deadline expires. Keep
/// an owned connection in this guard while changing transaction state so a
/// cancellation can never return an indeterminate session to the pool.
struct DiscardOnDropConnection(Option<TransactionConnection>);

impl Drop for DiscardOnDropConnection {
    fn drop(&mut self) {
        if let Some(connection) = &mut self.0 {
            connection.close_on_drop();
        }
    }
}

struct TransactionHandle {
    connection: Mutex<Option<TransactionConnection>>,
    default_timeout: Option<Duration>,
    sqlite_query_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SqlError {
    kind: &'static str,
    message: Box<str>,
    code: Option<Box<str>>,
    constraint: Option<Box<str>>,
    table: Option<Box<str>>,
    column: Option<Box<str>>,
}

impl SqlError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into().into_boxed_str(),
            code: None,
            constraint: None,
            table: None,
            column: None,
        }
    }

    fn connection(message: impl Into<String>) -> Self {
        Self::new("Connection", message)
    }

    fn closed(resource: &str) -> Self {
        Self::new("Closed", format!("{resource} is closed"))
    }

    fn decode(message: impl Into<String>, column: Option<String>) -> Self {
        Self {
            column: column.map(String::into_boxed_str),
            ..Self::new("Decode", message)
        }
    }

    fn unsupported(message: impl Into<String>) -> Self {
        Self::new("Unsupported", message)
    }

    fn timeout() -> Self {
        Self::new("Timeout", "SQL operation timed out")
    }

    fn into_external(self) -> BexExternalValue {
        BexExternalValue::Instance {
            class_name: "baml.sql.SqlError".to_string(),
            type_args: vec![],
            fields: IndexMap::from([
                (
                    "kind".to_string(),
                    BexExternalValue::variant("baml.sql.SqlErrorKind", self.kind),
                ),
                (
                    "message".to_string(),
                    BexExternalValue::String(self.message.to_string().into()),
                ),
                ("code".to_string(), optional_string(self.code)),
                ("constraint".to_string(), optional_string(self.constraint)),
                ("table".to_string(), optional_string(self.table)),
                ("column".to_string(), optional_string(self.column)),
            ]),
        }
    }
}

fn optional_string(value: Option<Box<str>>) -> BexExternalValue {
    value.map_or(BexExternalValue::Null, |value| {
        BexExternalValue::String(value.to_string().into())
    })
}

fn sql_output<T: Send + 'static>(
    future: impl Future<Output = Result<T, SqlError>> + Send + 'static,
) -> SysOpOutput<T> {
    SysOpOutput::async_op_with_throw(async move {
        future
            .await
            .map_err(|error| OpErrorBody::baml_thrown_value(error.into_external()))
    })
}

fn downcast_database(database: &owned::sql::Database) -> Result<Arc<DatabaseHandle>, SqlError> {
    database
        ._handle
        .clone()
        .downcast::<DatabaseHandle>()
        .map_err(|_| SqlError::closed("database"))
}

fn downcast_transaction(
    transaction: &owned::sql::Transaction,
) -> Result<Arc<TransactionHandle>, SqlError> {
    transaction
        ._handle
        .clone()
        .downcast::<TransactionHandle>()
        .map_err(|_| SqlError::closed("transaction"))
}

fn downcast_statement(statement: &owned::sql::Statement) -> Result<Arc<SqlStatement>, SqlError> {
    statement
        ._handle
        .clone()
        .downcast::<SqlStatement>()
        .map_err(|_| SqlError::unsupported("statement belongs to a different runtime"))
}

fn duration_value(value: Option<BexExternalValue>) -> Result<Option<Duration>, SqlError> {
    let Some(BexExternalValue::Instance { fields, .. }) = value else {
        return Ok(None);
    };
    let Some(BexExternalValue::Bigint(nanos)) = fields.get("_nanoseconds") else {
        return Err(SqlError::connection("invalid duration option"));
    };
    duration_bigint(nanos).map(Some)
}

fn duration_bigint(nanos: &BigInt) -> Result<Duration, SqlError> {
    duration_from_nanos(nanos, false)
}

fn nonnegative_duration_value(value: BexExternalValue) -> Result<Duration, SqlError> {
    let BexExternalValue::Instance { fields, .. } = value else {
        return Err(SqlError::connection("invalid duration option"));
    };
    let Some(BexExternalValue::Bigint(nanos)) = fields.get("_nanoseconds") else {
        return Err(SqlError::connection("invalid duration option"));
    };
    duration_from_nanos(nanos, true)
}

fn duration_from_nanos(nanos: &BigInt, allow_zero: bool) -> Result<Duration, SqlError> {
    if nanos.sign() == Sign::Minus || (!allow_zero && nanos.sign() == Sign::NoSign) {
        return Err(SqlError::connection(if allow_zero {
            "duration option must be nonnegative"
        } else {
            "duration options must be positive"
        }));
    }
    let billion = BigInt::from(1_000_000_000_u64);
    let seconds = (nanos / &billion)
        .to_u64()
        .ok_or_else(|| SqlError::connection("duration option is outside the supported range"))?;
    let subsecond_nanos = (nanos % billion)
        .to_u32()
        .expect("nonnegative nanosecond remainder fits u32");
    Ok(Duration::new(seconds, subsecond_nanos))
}

fn operation_timeout(
    explicit: Option<Arc<BigInt>>,
    default: Option<Duration>,
) -> Result<Option<Duration>, SqlError> {
    explicit
        .map(|value| duration_bigint(&value).map(Some))
        .unwrap_or(Ok(default))
}

async fn with_timeout<T>(
    timeout: Option<Duration>,
    future: impl Future<Output = Result<T, SqlError>>,
) -> Result<T, SqlError> {
    match timeout {
        Some(timeout) => tokio::time::timeout(timeout, future)
            .await
            .map_err(|_| SqlError::timeout())?,
        None => future.await,
    }
}

fn enum_variant(value: &BexExternalValue) -> Option<&str> {
    match value {
        BexExternalValue::Variant { variant_name, .. } => Some(variant_name),
        BexExternalValue::Union { value, .. } => enum_variant(value),
        _ => None,
    }
}

fn validate_count(value: Option<i64>, default: u32, name: &str) -> Result<u32, SqlError> {
    let value = value.unwrap_or(i64::from(default));
    u32::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| SqlError::connection(format!("{name} must be positive")))
}

fn database_value(handle: Arc<DatabaseHandle>) -> owned::sql::Database {
    let handle: Arc<dyn std::any::Any + Send + Sync> = handle;
    owned::sql::Database { _handle: handle }
}

fn database_external(handle: Arc<DatabaseHandle>) -> BexExternalValue {
    database_value(handle).into_bex_external_value()
}

async fn connect_postgres(
    url: String,
    options: owned::sql_postgres::PostgresOptions,
) -> Result<Arc<DatabaseHandle>, SqlError> {
    if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
        return Err(SqlError::connection(
            "PostgreSQL URLs must use postgres:// or postgresql://",
        ));
    }
    let max = validate_count(options.max_connections, 10, "max_connections")?;
    let min = u32::try_from(options.min_connections.unwrap_or(0))
        .map_err(|_| SqlError::connection("min_connections must be nonnegative"))?;
    if min > max {
        return Err(SqlError::connection(
            "min_connections must not exceed max_connections",
        ));
    }
    let connect_timeout = duration_value(options.connect_timeout)?;
    let query_timeout = duration_value(options.query_timeout)?;
    let idle_timeout = duration_value(options.idle_timeout)?;
    let max_lifetime = duration_value(options.max_lifetime)?;
    let mut connect = PgConnectOptions::from_str(&url)
        .map_err(|_| SqlError::connection("invalid PostgreSQL connection URL"))?;
    if let Some(name) = options.application_name {
        connect = connect.application_name(&name);
    }
    if let Some(mode) = options.ssl_mode.as_ref().and_then(enum_variant) {
        let explicit_rank = ssl_mode_rank(mode)
            .ok_or_else(|| SqlError::connection("invalid PostgreSQL SSL mode"))?;
        let url_rank = url::Url::parse(&url).ok().and_then(|url| {
            url.query_pairs()
                .filter(|(name, _)| name == "sslmode")
                .filter_map(|(_, value)| ssl_mode_rank(&value))
                .max()
        });
        if url_rank.is_some_and(|required| explicit_rank < required) {
            return Err(SqlError::connection(
                "ssl_mode must not weaken the PostgreSQL URL requirement",
            ));
        }
        let mode = match mode {
            "Disable" => PgSslMode::Disable,
            "Allow" => PgSslMode::Allow,
            "Prefer" => PgSslMode::Prefer,
            "Require" => PgSslMode::Require,
            "VerifyCa" => PgSslMode::VerifyCa,
            "VerifyFull" => PgSslMode::VerifyFull,
            _ => return Err(SqlError::connection("invalid PostgreSQL SSL mode")),
        };
        connect = connect.ssl_mode(mode);
    }
    let mut pool_options = PgPoolOptions::new()
        .max_connections(max)
        .min_connections(min);
    if let Some(value) = connect_timeout {
        pool_options = pool_options.acquire_timeout(value);
    }
    if let Some(value) = idle_timeout {
        pool_options = pool_options.idle_timeout(value);
    }
    if let Some(value) = max_lifetime {
        pool_options = pool_options.max_lifetime(value);
    }
    let pool = if options.validate.unwrap_or(true) {
        pool_options
            .connect_with(connect)
            .await
            .map_err(|error| sanitized_connection_error(error, "PostgreSQL"))?
    } else {
        pool_options.connect_lazy_with(connect)
    };
    Ok(Arc::new(DatabaseHandle {
        pool: DatabasePool::Postgres(pool),
        default_timeout: query_timeout,
        closed: AtomicBool::new(false),
    }))
}

fn ssl_mode_rank(mode: &str) -> Option<u8> {
    match mode.to_ascii_lowercase().replace(['-', '_'], "").as_str() {
        "disable" => Some(0),
        "allow" => Some(1),
        "prefer" => Some(2),
        "require" => Some(3),
        "verifyca" => Some(4),
        "verifyfull" => Some(5),
        _ => None,
    }
}

fn sqlite_url(input: &str) -> Result<String, SqlError> {
    if input == ":memory:" {
        return Ok("sqlite::memory:".to_string());
    }
    if let Some(path) = input.strip_prefix("sqlite://") {
        return Ok(format!("sqlite:{path}"));
    }
    if input.starts_with("sqlite:") {
        return Ok(input.to_string());
    }
    if let Some(path) = input.strip_prefix("file:") {
        return Ok(format!("sqlite:{path}"));
    }
    if input.contains("://") {
        return Err(SqlError::connection("unsupported SQLite URL scheme"));
    }
    Ok(format!("sqlite:{input}"))
}

async fn connect_sqlite(
    input: String,
    options: owned::sql_sqlite::SqliteOptions,
    force_memory: bool,
) -> Result<Arc<DatabaseHandle>, SqlError> {
    let max = validate_count(options.max_connections, 1, "max_connections")?;
    let mode = options.mode.as_ref().and_then(enum_variant);
    let journal = options.journal_mode.as_ref().and_then(enum_variant);
    let url = sqlite_url(if force_memory { ":memory:" } else { &input })?;
    let (url, uri_mode) = sqlite_mode_url(&url, mode)?;
    let uri_caches = url
        .split_once('?')
        .map(|(_, query)| {
            url::form_urlencoded::parse(query.as_bytes())
                .filter(|(key, _)| key == "cache")
                .map(|(_, value)| value.into_owned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if uri_mode.as_deref() == Some("memory") && uri_caches.as_slice() != ["private"] {
        return Err(SqlError::connection(
            "shared-cache SQLite memory databases are not supported",
        ));
    }
    let is_memory = force_memory
        || input == ":memory:"
        || input == "sqlite::memory:"
        || uri_mode.as_deref() == Some("memory");
    if is_memory {
        if max != 1 {
            return Err(SqlError::connection(
                "SQLite memory databases require max_connections = 1",
            ));
        }
        if matches!(mode, Some("ReadOnly" | "ReadWrite")) {
            return Err(SqlError::connection(
                "SQLite memory databases require ReadWriteCreate mode",
            ));
        }
        if journal == Some("Wal") {
            return Err(SqlError::connection(
                "SQLite memory databases do not support WAL mode",
            ));
        }
    }
    let mut connect = SqliteConnectOptions::from_str(&url)
        .map_err(|_| SqlError::connection("invalid SQLite connection URL"))?;
    connect = match mode {
        Some("ReadOnly") => connect.read_only(true).create_if_missing(false),
        Some("ReadWrite") => connect.read_only(false).create_if_missing(false),
        Some("ReadWriteCreate") => connect.read_only(false).create_if_missing(true),
        None if uri_mode.is_some() => connect,
        None => connect.read_only(false).create_if_missing(true),
        _ => return Err(SqlError::connection("invalid SQLite open mode")),
    };
    let busy = match options.busy_timeout {
        Some(value) => nonnegative_duration_value(value)?,
        None => Duration::from_secs(5),
    };
    connect = connect
        .busy_timeout(busy)
        .foreign_keys(options.foreign_keys.unwrap_or(true));
    if let Some(journal) = journal {
        let journal = match journal {
            "Delete" => SqliteJournalMode::Delete,
            "Truncate" => SqliteJournalMode::Truncate,
            "Persist" => SqliteJournalMode::Persist,
            "Memory" => SqliteJournalMode::Memory,
            "Wal" => SqliteJournalMode::Wal,
            "Off" => SqliteJournalMode::Off,
            _ => return Err(SqlError::connection("invalid SQLite journal mode")),
        };
        connect = connect.journal_mode(journal);
    }
    let pool_options = SqlitePoolOptions::new()
        .max_connections(max)
        .min_connections(1);
    let pool = if options.validate.unwrap_or(true) {
        pool_options
            .connect_with(connect)
            .await
            .map_err(|error| sanitized_connection_error(error, "SQLite"))?
    } else {
        pool_options.connect_lazy_with(connect)
    };
    Ok(Arc::new(DatabaseHandle {
        pool: DatabasePool::Sqlite(pool),
        default_timeout: duration_value(options.query_timeout)?,
        closed: AtomicBool::new(false),
    }))
}

fn sqlite_mode_url(
    url: &str,
    explicit: Option<&str>,
) -> Result<(String, Option<String>), SqlError> {
    let (base, query) = url.split_once('?').unwrap_or((url, ""));
    let mut pairs = url::form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    for key in ["mode", "cache", "immutable", "vfs"] {
        if pairs.iter().filter(|(name, _)| name == key).count() > 1 {
            return Err(SqlError::connection(format!(
                "conflicting SQLite URI {key} parameters"
            )));
        }
    }
    let modes = pairs
        .iter()
        .filter(|(key, _)| key == "mode")
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    let explicit = match explicit {
        Some("ReadOnly") => Some("ro"),
        Some("ReadWrite") => Some("rw"),
        Some("ReadWriteCreate") => Some("rwc"),
        Some(_) => return Err(SqlError::connection("invalid SQLite open mode")),
        None => None,
    };
    if let Some(explicit) = explicit {
        pairs.retain(|(key, _)| key != "mode");
        pairs.push(("mode".to_string(), explicit.to_string()));
    }
    let effective = explicit
        .map(ToOwned::to_owned)
        .or_else(|| modes.first().cloned());
    if pairs.is_empty() {
        return Ok((base.to_string(), effective));
    }
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.extend_pairs(pairs.iter().map(|(key, value)| (key, value)));
    Ok((format!("{base}?{}", serializer.finish()), effective))
}

fn map_sqlx_error(error: sqlx::Error, connecting: bool, timed_out: bool) -> SqlError {
    if timed_out {
        return SqlError::timeout();
    }
    let sqlx::Error::Database(database) = error else {
        return SqlError::new(
            if connecting { "Connection" } else { "Database" },
            if connecting {
                "failed to connect to SQL database"
            } else {
                "SQL database operation failed"
            },
        );
    };
    if let Some(pg) = database.try_downcast_ref::<sqlx::postgres::PgDatabaseError>() {
        let code = pg.code().to_string();
        let kind = if connecting || code.starts_with("08") {
            "Connection"
        } else if code.starts_with("23") {
            "Constraint"
        } else if code == "42601" {
            "Syntax"
        } else {
            "Database"
        };
        return SqlError {
            kind,
            message: pg.message().into(),
            code: Some(code.into_boxed_str()),
            constraint: pg.constraint().map(Into::into),
            table: pg.table().map(Into::into),
            column: pg.column().map(Into::into),
        };
    }
    let numeric = database.code().and_then(|code| code.parse::<i32>().ok());
    let primary = numeric.map(|code| code & 0xff);
    let kind = if connecting || primary == Some(14) {
        "Connection"
    } else if primary == Some(19) {
        "Constraint"
    } else if primary == Some(1) && database.message().to_ascii_lowercase().contains("syntax") {
        "Syntax"
    } else {
        "Database"
    };
    let code = numeric
        .and_then(sqlite_code_name)
        .or_else(|| primary.and_then(sqlite_code_name))
        .unwrap_or("SQLITE_ERROR");
    SqlError {
        kind,
        message: database.message().into(),
        code: Some(code.into()),
        constraint: database.constraint().map(Into::into),
        table: database.table().map(Into::into),
        column: None,
    }
}

fn sqlite_code_name(code: i32) -> Option<&'static str> {
    Some(match code {
        1 => "SQLITE_ERROR",
        2 => "SQLITE_INTERNAL",
        3 => "SQLITE_PERM",
        4 => "SQLITE_ABORT",
        5 => "SQLITE_BUSY",
        6 => "SQLITE_LOCKED",
        7 => "SQLITE_NOMEM",
        8 => "SQLITE_READONLY",
        9 => "SQLITE_INTERRUPT",
        10 => "SQLITE_IOERR",
        11 => "SQLITE_CORRUPT",
        12 => "SQLITE_NOTFOUND",
        13 => "SQLITE_FULL",
        14 => "SQLITE_CANTOPEN",
        15 => "SQLITE_PROTOCOL",
        16 => "SQLITE_EMPTY",
        17 => "SQLITE_SCHEMA",
        18 => "SQLITE_TOOBIG",
        19 => "SQLITE_CONSTRAINT",
        20 => "SQLITE_MISMATCH",
        21 => "SQLITE_MISUSE",
        22 => "SQLITE_NOLFS",
        23 => "SQLITE_AUTH",
        24 => "SQLITE_FORMAT",
        25 => "SQLITE_RANGE",
        26 => "SQLITE_NOTADB",
        261 => "SQLITE_BUSY_RECOVERY",
        517 => "SQLITE_BUSY_SNAPSHOT",
        773 => "SQLITE_BUSY_TIMEOUT",
        262 => "SQLITE_LOCKED_SHAREDCACHE",
        518 => "SQLITE_LOCKED_VTAB",
        264 => "SQLITE_READONLY_RECOVERY",
        520 => "SQLITE_READONLY_CANTLOCK",
        776 => "SQLITE_READONLY_ROLLBACK",
        1032 => "SQLITE_READONLY_DBMOVED",
        1288 => "SQLITE_READONLY_CANTINIT",
        1544 => "SQLITE_READONLY_DIRECTORY",
        270 => "SQLITE_CANTOPEN_NOTEMPDIR",
        526 => "SQLITE_CANTOPEN_ISDIR",
        782 => "SQLITE_CANTOPEN_FULLPATH",
        1038 => "SQLITE_CANTOPEN_CONVPATH",
        1294 => "SQLITE_CANTOPEN_DIRTYWAL",
        1550 => "SQLITE_CANTOPEN_SYMLINK",
        275 => "SQLITE_CONSTRAINT_CHECK",
        531 => "SQLITE_CONSTRAINT_COMMITHOOK",
        787 => "SQLITE_CONSTRAINT_FOREIGNKEY",
        1043 => "SQLITE_CONSTRAINT_FUNCTION",
        1299 => "SQLITE_CONSTRAINT_NOTNULL",
        1555 => "SQLITE_CONSTRAINT_PRIMARYKEY",
        1811 => "SQLITE_CONSTRAINT_TRIGGER",
        2067 => "SQLITE_CONSTRAINT_UNIQUE",
        2323 => "SQLITE_CONSTRAINT_VTAB",
        2579 => "SQLITE_CONSTRAINT_ROWID",
        2835 => "SQLITE_CONSTRAINT_PINNED",
        3091 => "SQLITE_CONSTRAINT_DATATYPE",
        _ => return None,
    })
}

fn sanitized_connection_error(error: sqlx::Error, provider: &str) -> SqlError {
    let mut error = map_sqlx_error(error, true, false);
    error.message = format!("failed to connect to {provider}").into_boxed_str();
    error.constraint = None;
    error.table = None;
    error.column = None;
    error
}

fn bind_error(message: impl Into<String>) -> SqlError {
    SqlError::unsupported(message)
}

fn bigint_i128(value: &BigInt, what: &str) -> Result<i128, SqlError> {
    value
        .to_i128()
        .ok_or_else(|| bind_error(format!("{what} is outside the supported SQL time range")))
}

fn truncate_to_microseconds(value: &BigInt, what: &str) -> Result<i128, SqlError> {
    bigint_i128(&(value / BigInt::from(1_000)), what)
}

fn offset_datetime(value: &BigInt, what: &str) -> Result<OffsetDateTime, SqlError> {
    let micros = truncate_to_microseconds(value, what)?;
    OffsetDateTime::from_unix_timestamp_nanos(micros * 1_000)
        .map_err(|_| bind_error(format!("{what} is outside the supported SQL time range")))
}

fn offset_datetime_exact(value: &BigInt, what: &str) -> Result<OffsetDateTime, SqlError> {
    OffsetDateTime::from_unix_timestamp_nanos(bigint_i128(value, what)?)
        .map_err(|_| bind_error(format!("{what} is outside the supported SQL time range")))
}

fn primitive_datetime(value: &BigInt, what: &str) -> Result<PrimitiveDateTime, SqlError> {
    Ok(PrimitiveDateTime::new(
        offset_datetime(value, what)?.date(),
        offset_datetime(value, what)?.time(),
    ))
}

fn primitive_datetime_exact(value: &BigInt, what: &str) -> Result<PrimitiveDateTime, SqlError> {
    let datetime = offset_datetime_exact(value, what)?;
    Ok(PrimitiveDateTime::new(datetime.date(), datetime.time()))
}

fn plain_date(days: i64) -> Result<Date, SqlError> {
    Date::from_ordinal_date(1970, 1)
        .expect("Unix epoch is valid")
        .checked_add(TimeDuration::days(days))
        .ok_or_else(|| bind_error("PlainDate is outside the supported SQL date range"))
}

fn plain_time(nanos: i64) -> Result<Time, SqlError> {
    if !(0..86_400_000_000_000).contains(&nanos) {
        return Err(bind_error(
            "PlainTime is outside the supported SQL time range",
        ));
    }
    let seconds = nanos / 1_000_000_000;
    Time::from_hms_nano(
        u8::try_from(seconds / 3_600).expect("PlainTime hours were range checked"),
        u8::try_from((seconds / 60) % 60).expect("PlainTime minutes were range checked"),
        u8::try_from(seconds % 60).expect("PlainTime seconds were range checked"),
        u32::try_from(nanos % 1_000_000_000).expect("PlainTime nanoseconds were range checked"),
    )
    .map_err(|_| bind_error("PlainTime is outside the supported SQL time range"))
}

fn sqlite_time_text(value: &SqlBindValue) -> Result<String, SqlError> {
    match value {
        SqlBindValue::Instant(value) => offset_datetime_exact(value, "Instant")?
            .format(&Rfc3339)
            .map_err(|_| bind_error("Instant could not be formatted")),
        SqlBindValue::ZonedDateTime {
            epoch_nanoseconds,
            offset_nanoseconds,
            iana,
        } => {
            let offset_nanoseconds = match (offset_nanoseconds, iana) {
                (Some(offset), _) => *offset,
                (None, Some(iana)) => {
                    let timezone = jiff::tz::TimeZone::get(iana)
                        .map_err(|_| bind_error("ZonedDateTime has an unknown IANA timezone"))?;
                    let timestamp = jiff::Timestamp::from_nanosecond(bigint_i128(
                        epoch_nanoseconds,
                        "ZonedDateTime",
                    )?)
                    .map_err(|_| {
                        bind_error("ZonedDateTime is outside the supported timezone range")
                    })?;
                    i64::from(timezone.to_offset(timestamp).seconds()) * 1_000_000_000
                }
                (None, None) => 0,
            };
            let seconds = offset_nanoseconds / 1_000_000_000;
            let seconds = i32::try_from(seconds)
                .map_err(|_| bind_error("ZonedDateTime offset is outside the supported range"))?;
            let offset = UtcOffset::from_whole_seconds(seconds)
                .map_err(|_| bind_error("ZonedDateTime offset is outside the supported range"))?;
            let mut text = offset_datetime_exact(epoch_nanoseconds, "ZonedDateTime")?
                .to_offset(offset)
                .format(&Rfc3339)
                .map_err(|_| bind_error("ZonedDateTime could not be formatted"))?;
            if let Some(iana) = iana {
                text.push('[');
                text.push_str(iana);
                text.push(']');
            }
            Ok(text)
        }
        SqlBindValue::PlainDateTime(value) => primitive_datetime_exact(value, "PlainDateTime")?
            .format(PLAIN_DATETIME_FORMAT)
            .map_err(|_| bind_error("PlainDateTime could not be formatted")),
        SqlBindValue::PlainDate(value) => Ok(plain_date(*value)?.to_string()),
        SqlBindValue::PlainTime(value) => plain_time(*value)?
            .format(PLAIN_TIME_FORMAT)
            .map_err(|_| bind_error("PlainTime could not be formatted")),
        SqlBindValue::Duration(value) => Ok(value.to_string()),
        _ => unreachable!("called only for SQL time binds"),
    }
}

fn pg_arguments(values: &[SqlBindValue]) -> Result<PgArguments, SqlError> {
    let mut arguments = PgArguments::default();
    for value in values {
        match value {
            SqlBindValue::Null => arguments.add(Option::<String>::None),
            SqlBindValue::Bool(value) => arguments.add(*value),
            SqlBindValue::Int(value) => arguments.add(*value),
            SqlBindValue::BigInt(value) => arguments.add(
                BigDecimal::from_str(&value.to_string())
                    .map_err(|_| bind_error("invalid bigint SQL bind"))?,
            ),
            SqlBindValue::Float(value) => arguments.add(*value),
            SqlBindValue::String(value) => arguments.add(value.clone()),
            SqlBindValue::Bytes(value) => arguments.add(value.clone()),
            SqlBindValue::Json(value) => arguments.add(Json(value.clone())),
            SqlBindValue::Instant(value) => arguments.add(offset_datetime(value, "Instant")?),
            SqlBindValue::ZonedDateTime {
                epoch_nanoseconds, ..
            } => arguments.add(offset_datetime(epoch_nanoseconds, "ZonedDateTime")?),
            SqlBindValue::PlainDateTime(value) => {
                arguments.add(primitive_datetime(value, "PlainDateTime")?)
            }
            SqlBindValue::PlainDate(value) => arguments.add(plain_date(*value)?),
            SqlBindValue::PlainTime(value) => arguments.add(plain_time(*value)?),
            SqlBindValue::Duration(value) => {
                let micros = truncate_to_microseconds(value, "Duration")?;
                let microseconds = i64::try_from(micros)
                    .map_err(|_| bind_error("Duration is outside PostgreSQL interval range"))?;
                arguments.add(PgInterval {
                    months: 0,
                    days: 0,
                    microseconds,
                })
            }
            SqlBindValue::Array {
                element_type,
                values,
            } => Ok(add_pg_array(&mut arguments, *element_type, values)?),
        }
        .map_err(|_| bind_error("SQL bind could not be encoded"))?;
    }
    Ok(arguments)
}

fn collect_pg_array<T>(
    values: &[SqlBindValue],
    map: impl Fn(&SqlBindValue) -> Result<T, SqlError>,
) -> Result<Vec<Option<T>>, SqlError> {
    values
        .iter()
        .map(|value| {
            if matches!(value, SqlBindValue::Null) {
                Ok(None)
            } else {
                map(value).map(Some)
            }
        })
        .collect()
}

fn pg_interval(value: &BigInt) -> Result<PgInterval, SqlError> {
    let micros = truncate_to_microseconds(value, "Duration")?;
    Ok(PgInterval {
        months: 0,
        days: 0,
        microseconds: i64::try_from(micros)
            .map_err(|_| bind_error("Duration is outside PostgreSQL interval range"))?,
    })
}

fn add_pg_array(
    arguments: &mut PgArguments,
    element_type: SqlArrayType,
    values: &[SqlBindValue],
) -> Result<(), SqlError> {
    macro_rules! add {
        ($values:expr) => {
            arguments
                .add($values)
                .map_err(|_| bind_error("PostgreSQL array bind could not be encoded"))
        };
    }
    match element_type {
        SqlArrayType::Bool => add!(collect_pg_array(values, |value| match value {
            SqlBindValue::Bool(value) => Ok(*value),
            _ => Err(bind_error("SQL array binds must be homogeneous")),
        })?),
        SqlArrayType::Int => add!(collect_pg_array(values, |value| match value {
            SqlBindValue::Int(value) => Ok(*value),
            _ => Err(bind_error("SQL array binds must be homogeneous")),
        })?),
        SqlArrayType::BigInt => add!(collect_pg_array(values, |value| match value {
            SqlBindValue::BigInt(value) => BigDecimal::from_str(&value.to_string())
                .map_err(|_| bind_error("invalid bigint SQL array bind")),
            _ => Err(bind_error("SQL array binds must be homogeneous")),
        })?),
        SqlArrayType::Float => add!(collect_pg_array(values, |value| match value {
            SqlBindValue::Float(value) => Ok(*value),
            _ => Err(bind_error("SQL array binds must be homogeneous")),
        })?),
        SqlArrayType::String => add!(collect_pg_array(values, |value| match value {
            SqlBindValue::String(value) => Ok(value.clone()),
            _ => Err(bind_error("SQL array binds must be homogeneous")),
        })?),
        SqlArrayType::Bytes => add!(collect_pg_array(values, |value| match value {
            SqlBindValue::Bytes(value) => Ok(value.clone()),
            _ => Err(bind_error("SQL array binds must be homogeneous")),
        })?),
        SqlArrayType::Json => add!(collect_pg_array(values, |value| match value {
            SqlBindValue::Json(value) => Ok(Json(value.clone())),
            _ => Err(bind_error("SQL array binds must be homogeneous")),
        })?),
        SqlArrayType::Instant => add!(collect_pg_array(values, |value| match value {
            SqlBindValue::Instant(value) => offset_datetime(value, "Instant"),
            _ => Err(bind_error("SQL array binds must be homogeneous")),
        })?),
        SqlArrayType::ZonedDateTime => add!(collect_pg_array(values, |value| match value {
            SqlBindValue::ZonedDateTime {
                epoch_nanoseconds, ..
            } => offset_datetime(epoch_nanoseconds, "ZonedDateTime"),
            _ => Err(bind_error("SQL array binds must be homogeneous")),
        })?),
        SqlArrayType::PlainDateTime => add!(collect_pg_array(values, |value| match value {
            SqlBindValue::PlainDateTime(value) => primitive_datetime(value, "PlainDateTime"),
            _ => Err(bind_error("SQL array binds must be homogeneous")),
        })?),
        SqlArrayType::PlainDate => add!(collect_pg_array(values, |value| match value {
            SqlBindValue::PlainDate(value) => plain_date(*value),
            _ => Err(bind_error("SQL array binds must be homogeneous")),
        })?),
        SqlArrayType::PlainTime => add!(collect_pg_array(values, |value| match value {
            SqlBindValue::PlainTime(value) => plain_time(*value),
            _ => Err(bind_error("SQL array binds must be homogeneous")),
        })?),
        SqlArrayType::Duration => add!(collect_pg_array(values, |value| match value {
            SqlBindValue::Duration(value) => pg_interval(value),
            _ => Err(bind_error("SQL array binds must be homogeneous")),
        })?),
    }
}

fn sqlite_arguments(values: &[SqlBindValue]) -> Result<SqliteArguments<'static>, SqlError> {
    let mut arguments = SqliteArguments::default();
    for value in values {
        match value {
            SqlBindValue::Null => arguments.add(Option::<String>::None),
            SqlBindValue::Bool(value) => arguments.add(i64::from(*value)),
            SqlBindValue::Int(value) => arguments.add(*value),
            SqlBindValue::BigInt(value) => match value.to_i64() {
                Some(value) => arguments.add(value),
                None => arguments.add(value.to_string()),
            },
            SqlBindValue::Float(value) => arguments.add(*value),
            SqlBindValue::String(value) => arguments.add(value.clone()),
            SqlBindValue::Bytes(value) => arguments.add(value.clone()),
            SqlBindValue::Json(value) => arguments.add(
                serde_json::to_string(value)
                    .map_err(|_| bind_error("JSON SQL bind could not be encoded"))?,
            ),
            value @ (SqlBindValue::Instant(_)
            | SqlBindValue::ZonedDateTime { .. }
            | SqlBindValue::PlainDateTime(_)
            | SqlBindValue::PlainDate(_)
            | SqlBindValue::PlainTime(_)
            | SqlBindValue::Duration(_)) => arguments.add(sqlite_time_text(value)?),
            SqlBindValue::Array { .. } => {
                return Err(bind_error("SQLite does not support array binds"));
            }
        }
        .map_err(|_| bind_error("SQL bind could not be encoded"))?;
    }
    Ok(arguments)
}

#[derive(Clone)]
struct DecodeContext {
    classes: Arc<IndexMap<baml_type::TypeName, sys_types::ClassDefinition>>,
    aliases: Arc<IndexMap<baml_type::TypeName, RuntimeTy>>,
}

impl DecodeContext {
    fn new(ctx: &SysOpContext) -> Self {
        Self {
            classes: ctx.class_definitions.clone(),
            aliases: ctx.type_alias_definitions.clone(),
        }
    }
}

fn decode_failure(column: &str, expected: &RuntimeTy) -> SqlError {
    SqlError::decode(
        format!("column {column:?} cannot be decoded as {expected}"),
        Some(column.to_string()),
    )
}

fn nullable_target(ty: &RuntimeTy) -> (&RuntimeTy, bool) {
    if let RuntimeTy::Union(members, _) = ty {
        let mut non_null = members
            .iter()
            .filter(|member| !matches!(member, RuntimeTy::Null { .. }));
        if let Some(inner) = non_null.next()
            && non_null.next().is_none()
            && members.len() == 2
        {
            return (inner, true);
        }
    }
    (ty, matches!(ty, RuntimeTy::Null { .. }))
}

fn resolve_alias<'a>(ty: &'a RuntimeTy, ctx: &'a DecodeContext) -> &'a RuntimeTy {
    let mut current = ty;
    for _ in 0..32 {
        let RuntimeTy::TypeAlias(name, _) = current else {
            break;
        };
        let Some(target) = ctx.aliases.get(name) else {
            break;
        };
        current = target;
    }
    current
}

fn class_name(ty: &RuntimeTy) -> Option<&baml_type::TypeName> {
    match ty {
        RuntimeTy::Class(name, _, _) => Some(name),
        _ => None,
    }
}

fn external_instance(
    class_name: &str,
    fields: impl IntoIterator<Item = (String, BexExternalValue)>,
) -> BexExternalValue {
    BexExternalValue::Instance {
        class_name: class_name.to_string(),
        type_args: vec![],
        fields: fields.into_iter().collect(),
    }
}

fn bigint_value(value: impl Into<BigInt>) -> BexExternalValue {
    BexExternalValue::Bigint(value.into())
}

fn instant_value(value: OffsetDateTime) -> BexExternalValue {
    external_instance(
        "baml.time.Instant",
        [(
            "_nanoseconds".to_string(),
            bigint_value(value.unix_timestamp_nanos()),
        )],
    )
}

fn zoned_value(value: OffsetDateTime) -> BexExternalValue {
    external_instance(
        "baml.time.ZonedDateTime",
        [
            (
                "_nanoseconds".to_string(),
                bigint_value(value.unix_timestamp_nanos()),
            ),
            ("_offset_ns".to_string(), BexExternalValue::Int(0)),
            ("_iana".to_string(), BexExternalValue::Null),
        ],
    )
}

fn plain_datetime_value(value: PrimitiveDateTime) -> BexExternalValue {
    external_instance(
        "baml.time.PlainDateTime",
        [(
            "_nanoseconds".to_string(),
            bigint_value(value.assume_utc().unix_timestamp_nanos()),
        )],
    )
}

fn plain_date_value(value: Date) -> BexExternalValue {
    let epoch = Date::from_ordinal_date(1970, 1).expect("Unix epoch is valid");
    external_instance(
        "baml.time.PlainDate",
        [(
            "_days".to_string(),
            BexExternalValue::Int((value - epoch).whole_days()),
        )],
    )
}

fn plain_time_value(value: Time) -> BexExternalValue {
    let nanos = i64::from(value.hour()) * 3_600_000_000_000
        + i64::from(value.minute()) * 60_000_000_000
        + i64::from(value.second()) * 1_000_000_000
        + i64::from(value.nanosecond());
    external_instance(
        "baml.time.PlainTime",
        [("_nanoseconds".to_string(), BexExternalValue::Int(nanos))],
    )
}

fn duration_value_external(nanos: BigInt) -> BexExternalValue {
    external_instance(
        "baml.time.Duration",
        [("_nanoseconds".to_string(), BexExternalValue::Bigint(nanos))],
    )
}

fn json_external(value: serde_json::Value) -> BexExternalValue {
    match value {
        serde_json::Value::Null => BexExternalValue::Null,
        serde_json::Value::Bool(value) => BexExternalValue::Bool(value),
        serde_json::Value::Number(value) => value.as_i64().map_or_else(
            || BexExternalValue::Float(value.as_f64().expect("JSON number is finite")),
            BexExternalValue::Int,
        ),
        serde_json::Value::String(value) => BexExternalValue::String(value.into()),
        serde_json::Value::Array(values) => BexExternalValue::Array {
            element_type: RuntimeTy::BuiltinUnknown {
                attr: baml_type::TyAttr::default(),
            },
            items: values.into_iter().map(json_external).collect(),
        },
        serde_json::Value::Object(values) => BexExternalValue::Map {
            key_type: RuntimeTy::String {
                attr: baml_type::TyAttr::default(),
            },
            value_type: RuntimeTy::BuiltinUnknown {
                attr: baml_type::TyAttr::default(),
            },
            entries: values
                .into_iter()
                .map(|(key, value)| (key, json_external(value)))
                .collect(),
        },
    }
}

fn is_json_type(ty: &RuntimeTy) -> bool {
    matches!(ty, RuntimeTy::TypeAlias(name, _) if name.to_string() == "baml.json.json")
}

fn decode_pg<'r, T>(raw: PgValueRef<'r>) -> Result<T, sqlx::error::BoxDynError>
where
    T: Decode<'r, Postgres>,
{
    T::decode(raw)
}

fn pg_decode_raw(
    raw: PgValueRef<'_>,
    ty: &RuntimeTy,
    ctx: &DecodeContext,
    column: &str,
) -> Result<BexExternalValue, SqlError> {
    let (ty, nullable) = nullable_target(ty);
    if raw.is_null() {
        return if nullable || matches!(ty, RuntimeTy::Null { .. }) {
            Ok(BexExternalValue::Null)
        } else {
            Err(decode_failure(column, ty))
        };
    }
    if is_json_type(ty) {
        return <Json<serde_json::Value> as Decode<Postgres>>::decode(raw)
            .map(|value| json_external(value.0))
            .map_err(|_| decode_failure(column, ty));
    }
    let ty = resolve_alias(ty, ctx);
    let source = raw.type_info().name().to_ascii_uppercase();
    let failed = || decode_failure(column, ty);
    match ty {
        RuntimeTy::Bool { .. } if source == "BOOL" => decode_pg::<bool>(raw)
            .map(BexExternalValue::Bool)
            .map_err(|_| failed()),
        RuntimeTy::Int { .. } => match source.as_str() {
            "INT2" => decode_pg::<i16>(raw).map(i64::from),
            "INT4" => decode_pg::<i32>(raw).map(i64::from),
            "INT8" => decode_pg::<i64>(raw),
            _ => return Err(failed()),
        }
        .map(BexExternalValue::Int)
        .map_err(|_| failed()),
        RuntimeTy::Bigint { .. } => match source.as_str() {
            "INT2" => decode_pg::<i16>(raw).map(BigInt::from),
            "INT4" => decode_pg::<i32>(raw).map(BigInt::from),
            "INT8" => decode_pg::<i64>(raw).map(BigInt::from),
            "NUMERIC" => BigDecimal::decode(raw).and_then(|value| {
                value
                    .to_bigint()
                    .filter(|integer| BigDecimal::from(integer.clone()) == value)
                    .ok_or_else(|| "numeric has a fractional component".into())
            }),
            _ => return Err(failed()),
        }
        .map(BexExternalValue::Bigint)
        .map_err(|_| failed()),
        RuntimeTy::Float { .. } => match source.as_str() {
            "FLOAT4" => decode_pg::<f32>(raw).map(f64::from),
            "FLOAT8" => decode_pg::<f64>(raw),
            "NUMERIC" => BigDecimal::decode(raw).and_then(|value| {
                value
                    .to_f64()
                    .ok_or_else(|| "numeric is outside float range".into())
            }),
            _ => return Err(failed()),
        }
        .map(BexExternalValue::Float)
        .map_err(|_| failed()),
        RuntimeTy::String { .. } => {
            let value = match source.as_str() {
                "NUMERIC" => BigDecimal::decode(raw).map(|value| value.normalized().to_string()),
                "UUID" => decode_pg::<uuid::Uuid>(raw).map(|value| value.to_string()),
                "DATE" => decode_pg::<Date>(raw).map(|value| value.to_string()),
                "TIME" => decode_pg::<Time>(raw).map(|value| value.to_string()),
                "TIMESTAMP" => decode_pg::<PrimitiveDateTime>(raw).map(|value| value.to_string()),
                "TIMESTAMPTZ" => decode_pg::<OffsetDateTime>(raw).map(|value| value.to_string()),
                "INTERVAL" => PgInterval::decode(raw).map(|value| {
                    format!(
                        "{} mons {} days {} microseconds",
                        value.months, value.days, value.microseconds
                    )
                }),
                _ => decode_pg::<String>(raw),
            };
            value
                .map(|value| BexExternalValue::String(value.into()))
                .map_err(|_| failed())
        }
        RuntimeTy::Uint8Array { .. } if source == "BYTEA" => decode_pg::<Vec<u8>>(raw)
            .map(BexExternalValue::Uint8Array)
            .map_err(|_| failed()),
        RuntimeTy::List(element, _) => pg_decode_array(raw, element, ctx, column),
        RuntimeTy::Class(name, _, _) => match name.to_string().as_str() {
            "baml.time.Instant" if source == "TIMESTAMPTZ" => decode_pg::<OffsetDateTime>(raw)
                .map(instant_value)
                .map_err(|_| failed()),
            "baml.time.ZonedDateTime" if source == "TIMESTAMPTZ" => {
                decode_pg::<OffsetDateTime>(raw)
                    .map(zoned_value)
                    .map_err(|_| failed())
            }
            "baml.time.PlainDateTime" if source == "TIMESTAMP" => {
                decode_pg::<PrimitiveDateTime>(raw)
                    .map(plain_datetime_value)
                    .map_err(|_| failed())
            }
            "baml.time.PlainDate" if source == "DATE" => decode_pg::<Date>(raw)
                .map(plain_date_value)
                .map_err(|_| failed()),
            "baml.time.PlainTime" if source == "TIME" => decode_pg::<Time>(raw)
                .map(plain_time_value)
                .map_err(|_| failed()),
            "baml.time.Duration" if source == "INTERVAL" => PgInterval::decode(raw)
                .and_then(|value| {
                    if value.months != 0 {
                        Err("calendar-month intervals are not fixed durations".into())
                    } else {
                        Ok(duration_value_external(
                            (BigInt::from(value.days) * BigInt::from(86_400_000_000_i64)
                                + BigInt::from(value.microseconds))
                                * BigInt::from(1_000),
                        ))
                    }
                })
                .map_err(|_| failed()),
            _ => Err(failed()),
        },
        RuntimeTy::Null { .. } => Err(failed()),
        _ => Err(failed()),
    }
}

fn pg_decode_array(
    raw: PgValueRef<'_>,
    element: &RuntimeTy,
    ctx: &DecodeContext,
    column: &str,
) -> Result<BexExternalValue, SqlError> {
    let source = raw.type_info().name().to_ascii_uppercase();
    let (inner, nullable) = nullable_target(element);
    if is_json_type(inner) {
        let items = decode_pg::<Vec<Option<Json<serde_json::Value>>>>(raw)
            .map_err(|_| decode_failure(column, element))?
            .into_iter()
            .map(|value| match value {
                Some(value) => Ok(json_external(value.0)),
                None if nullable => Ok(BexExternalValue::Null),
                None => Err(decode_failure(column, element)),
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(BexExternalValue::Array {
            element_type: element.clone(),
            items,
        });
    }
    let inner = resolve_alias(inner, ctx);
    let failed = || decode_failure(column, element);
    macro_rules! decode_array {
        ($source_ty:ty, $map:expr) => {{
            decode_pg::<Vec<Option<$source_ty>>>(raw.clone())
                .map_err(|_| failed())?
                .into_iter()
                .map(|value| match value {
                    Some(value) => Ok(($map)(value)),
                    None if nullable => Ok(BexExternalValue::Null),
                    None => Err(failed()),
                })
                .collect::<Result<Vec<_>, _>>()?
        }};
    }
    let items = match inner {
        RuntimeTy::Bool { .. } => decode_array!(bool, BexExternalValue::Bool),
        RuntimeTy::Int { .. } => match source.as_str() {
            "INT2[]" => decode_array!(i16, |value| BexExternalValue::Int(i64::from(value))),
            "INT4[]" => decode_array!(i32, |value| BexExternalValue::Int(i64::from(value))),
            "INT8[]" => decode_array!(i64, BexExternalValue::Int),
            _ => return Err(failed()),
        },
        RuntimeTy::Bigint { .. } => match source.as_str() {
            "INT2[]" => decode_array!(i16, bigint_value),
            "INT4[]" => decode_array!(i32, bigint_value),
            "INT8[]" => decode_array!(i64, bigint_value),
            "NUMERIC[]" => decode_pg::<Vec<Option<BigDecimal>>>(raw.clone())
                .map_err(|_| failed())?
                .into_iter()
                .map(|value| match value {
                    Some(value) => value
                        .to_bigint()
                        .filter(|integer| BigDecimal::from(integer.clone()) == value)
                        .map(BexExternalValue::Bigint)
                        .ok_or_else(&failed),
                    None if nullable => Ok(BexExternalValue::Null),
                    None => Err(failed()),
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(failed()),
        },
        RuntimeTy::Float { .. } => match source.as_str() {
            "FLOAT4[]" => decode_array!(f32, |value| BexExternalValue::Float(f64::from(value))),
            "FLOAT8[]" => decode_array!(f64, BexExternalValue::Float),
            "NUMERIC[]" => decode_array!(BigDecimal, |value: BigDecimal| {
                BexExternalValue::Float(value.to_f64().unwrap_or(f64::NAN))
            }),
            _ => return Err(failed()),
        },
        RuntimeTy::String { .. } => match source.as_str() {
            "NUMERIC[]" => decode_array!(BigDecimal, |value: BigDecimal| {
                BexExternalValue::String(value.normalized().to_string().into())
            }),
            "UUID[]" => decode_array!(uuid::Uuid, |value: uuid::Uuid| {
                BexExternalValue::String(value.to_string().into())
            }),
            "DATE[]" => decode_array!(Date, |value: Date| {
                BexExternalValue::String(value.to_string().into())
            }),
            "TIME[]" => decode_array!(Time, |value: Time| {
                BexExternalValue::String(value.to_string().into())
            }),
            "TIMESTAMP[]" => decode_array!(PrimitiveDateTime, |value: PrimitiveDateTime| {
                BexExternalValue::String(value.to_string().into())
            }),
            "TIMESTAMPTZ[]" => decode_array!(OffsetDateTime, |value: OffsetDateTime| {
                BexExternalValue::String(value.to_string().into())
            }),
            "INTERVAL[]" => decode_array!(PgInterval, |value: PgInterval| {
                BexExternalValue::String(
                    format!(
                        "{} mons {} days {} microseconds",
                        value.months, value.days, value.microseconds
                    )
                    .into(),
                )
            }),
            _ => decode_array!(String, |value: String| BexExternalValue::String(
                value.into()
            )),
        },
        RuntimeTy::Uint8Array { .. } => decode_array!(Vec<u8>, BexExternalValue::Uint8Array),
        RuntimeTy::Class(name, _, _) => match (name.to_string().as_str(), source.as_str()) {
            ("baml.time.Instant", "TIMESTAMPTZ[]") => {
                decode_array!(OffsetDateTime, instant_value)
            }
            ("baml.time.ZonedDateTime", "TIMESTAMPTZ[]") => {
                decode_array!(OffsetDateTime, zoned_value)
            }
            ("baml.time.PlainDateTime", "TIMESTAMP[]") => {
                decode_array!(PrimitiveDateTime, plain_datetime_value)
            }
            ("baml.time.PlainDate", "DATE[]") => decode_array!(Date, plain_date_value),
            ("baml.time.PlainTime", "TIME[]") => decode_array!(Time, plain_time_value),
            ("baml.time.Duration", "INTERVAL[]") => {
                let values =
                    decode_pg::<Vec<Option<PgInterval>>>(raw.clone()).map_err(|_| failed())?;
                values
                    .into_iter()
                    .map(|value| match value {
                        Some(value) if value.months == 0 => Ok(duration_value_external(
                            (BigInt::from(value.days) * BigInt::from(86_400_000_000_i64)
                                + BigInt::from(value.microseconds))
                                * BigInt::from(1_000),
                        )),
                        Some(_) => Err(failed()),
                        None if nullable => Ok(BexExternalValue::Null),
                        None => Err(failed()),
                    })
                    .collect::<Result<Vec<_>, _>>()?
            }
            _ => return Err(failed()),
        },
        _ => return Err(failed()),
    };
    if matches!(inner, RuntimeTy::Float { .. })
        && items
            .iter()
            .any(|value| matches!(value, BexExternalValue::Float(v) if !v.is_finite()))
    {
        return Err(failed());
    }
    Ok(BexExternalValue::Array {
        element_type: element.clone(),
        items,
    })
}

fn sqlite_decode_raw(
    raw: SqliteValueRef<'_>,
    ty: &RuntimeTy,
    ctx: &DecodeContext,
    column: &str,
) -> Result<BexExternalValue, SqlError> {
    let (ty, nullable) = nullable_target(ty);
    if raw.is_null() {
        return if nullable || matches!(ty, RuntimeTy::Null { .. }) {
            Ok(BexExternalValue::Null)
        } else {
            Err(decode_failure(column, ty))
        };
    }
    let source = raw.type_info().name().to_ascii_uppercase();
    if is_json_type(ty) {
        return if source == "TEXT" {
            <String as Decode<Sqlite>>::decode(raw)
                .ok()
                .and_then(|value| serde_json::from_str(&value).ok())
                .map(json_external)
                .ok_or_else(|| decode_failure(column, ty))
        } else {
            Err(decode_failure(column, ty))
        };
    }
    let ty = resolve_alias(ty, ctx);
    let failed = || decode_failure(column, ty);
    match ty {
        RuntimeTy::Bool { .. } if source == "INTEGER" => <i64 as Decode<Sqlite>>::decode(raw)
            .ok()
            .and_then(|value| match value {
                0 => Some(false),
                1 => Some(true),
                _ => None,
            })
            .map(BexExternalValue::Bool)
            .ok_or_else(failed),
        RuntimeTy::Int { .. } if source == "INTEGER" => <i64 as Decode<Sqlite>>::decode(raw)
            .map(BexExternalValue::Int)
            .map_err(|_| failed()),
        RuntimeTy::Bigint { .. } => match source.as_str() {
            "INTEGER" => <i64 as Decode<Sqlite>>::decode(raw).map(BigInt::from),
            "TEXT" => <String as Decode<Sqlite>>::decode(raw).and_then(|value| {
                let integer = BigInt::from_str(&value)?;
                if integer.to_string() == value {
                    Ok(integer)
                } else {
                    Err("integer text is not canonical".into())
                }
            }),
            _ => return Err(failed()),
        }
        .map(BexExternalValue::Bigint)
        .map_err(|_| failed()),
        RuntimeTy::Float { .. } if source == "REAL" => <f64 as Decode<Sqlite>>::decode(raw)
            .map(BexExternalValue::Float)
            .map_err(|_| failed()),
        RuntimeTy::String { .. } if source == "TEXT" => <String as Decode<Sqlite>>::decode(raw)
            .map(|value| BexExternalValue::String(value.into()))
            .map_err(|_| failed()),
        RuntimeTy::Uint8Array { .. } if source == "BLOB" => {
            <Vec<u8> as Decode<Sqlite>>::decode(raw)
                .map(BexExternalValue::Uint8Array)
                .map_err(|_| failed())
        }
        RuntimeTy::Class(name, _, _) if source == "TEXT" => {
            let text = <String as Decode<Sqlite>>::decode(raw).map_err(|_| failed())?;
            match name.to_string().as_str() {
                "baml.time.Instant" => OffsetDateTime::parse(&text, &Rfc3339)
                    .map(instant_value)
                    .map_err(|_| failed()),
                "baml.time.ZonedDateTime" => {
                    let (timestamp, iana) = text
                        .rfind('[')
                        .filter(|_| text.ends_with(']'))
                        .map_or((text.as_str(), None), |index| {
                            (&text[..index], Some(&text[index + 1..text.len() - 1]))
                        });
                    let value = OffsetDateTime::parse(timestamp, &Rfc3339).map_err(|_| failed())?;
                    Ok(external_instance(
                        "baml.time.ZonedDateTime",
                        [
                            (
                                "_nanoseconds".to_string(),
                                bigint_value(value.unix_timestamp_nanos()),
                            ),
                            (
                                "_offset_ns".to_string(),
                                if iana.is_some() {
                                    BexExternalValue::Null
                                } else {
                                    BexExternalValue::Int(
                                        i64::from(value.offset().whole_seconds()) * 1_000_000_000,
                                    )
                                },
                            ),
                            (
                                "_iana".to_string(),
                                iana.map_or(BexExternalValue::Null, |value| {
                                    BexExternalValue::String(value.to_string().into())
                                }),
                            ),
                        ],
                    ))
                }
                "baml.time.PlainDateTime" => PrimitiveDateTime::parse(&text, PLAIN_DATETIME_FORMAT)
                    .map(plain_datetime_value)
                    .map_err(|_| failed()),
                "baml.time.PlainDate" => Date::parse(&text, &Iso8601::DATE)
                    .map(plain_date_value)
                    .map_err(|_| failed()),
                "baml.time.PlainTime" => Time::parse(&text, PLAIN_TIME_FORMAT)
                    .map(plain_time_value)
                    .map_err(|_| failed()),
                "baml.time.Duration" => BigInt::from_str(&text)
                    .map(duration_value_external)
                    .map_err(|_| failed()),
                _ => Err(failed()),
            }
        }
        RuntimeTy::Null { .. } => Err(failed()),
        _ => Err(failed()),
    }
}

fn decode_pg_row(
    row: &PgRow,
    ty: &RuntimeTy,
    ctx: &DecodeContext,
) -> Result<BexExternalValue, SqlError> {
    let resolved = resolve_alias(ty, ctx);
    if let Some(name) = class_name(resolved)
        && !name.to_string().starts_with("baml.")
    {
        return decode_pg_class(row, name, ctx);
    }
    if row.len() != 1 {
        return Err(SqlError::decode(
            "non-class query results require exactly one column",
            None,
        ));
    }
    let column = row.columns()[0].name();
    pg_decode_raw(
        row.try_get_raw(0)
            .map_err(|_| SqlError::decode("could not read result column", Some(column.into())))?,
        ty,
        ctx,
        column,
    )
}

fn decode_sqlite_row(
    row: &SqliteRow,
    ty: &RuntimeTy,
    ctx: &DecodeContext,
) -> Result<BexExternalValue, SqlError> {
    let resolved = resolve_alias(ty, ctx);
    if let Some(name) = class_name(resolved)
        && !name.to_string().starts_with("baml.")
    {
        return decode_sqlite_class(row, name, ctx);
    }
    if row.len() != 1 {
        return Err(SqlError::decode(
            "non-class query results require exactly one column",
            None,
        ));
    }
    let column = row.columns()[0].name();
    sqlite_decode_raw(
        row.try_get_raw(0)
            .map_err(|_| SqlError::decode("could not read result column", Some(column.into())))?,
        ty,
        ctx,
        column,
    )
}

fn matching_column<R: Row>(row: &R, serialized: &str) -> Result<Option<usize>, SqlError>
where
    usize: sqlx::ColumnIndex<R>,
{
    let matches = row
        .columns()
        .iter()
        .enumerate()
        .filter_map(|(index, column)| (column.name() == serialized).then_some(index))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [index] => Ok(Some(*index)),
        _ => Err(SqlError::decode(
            format!("multiple result columns match field {serialized:?}"),
            Some(serialized.to_string()),
        )),
    }
}

fn decode_pg_class(
    row: &PgRow,
    name: &baml_type::TypeName,
    ctx: &DecodeContext,
) -> Result<BexExternalValue, SqlError> {
    let definition = ctx
        .classes
        .get(name)
        .ok_or_else(|| SqlError::decode(format!("unknown BAML class {name}"), None))?;
    let mut fields = IndexMap::new();
    for field in &definition.fields {
        let serialized = field.alias.as_deref().unwrap_or(&field.name);
        let value = match matching_column(row, serialized)? {
            Some(index) => pg_decode_raw(
                row.try_get_raw(index).map_err(|_| {
                    SqlError::decode("could not read result column", Some(serialized.into()))
                })?,
                &field.field_type,
                ctx,
                serialized,
            )?,
            None if nullable_target(&field.field_type).1 => BexExternalValue::Null,
            None => {
                return Err(SqlError::decode(
                    format!("required result column {serialized:?} is missing"),
                    Some(serialized.to_string()),
                ));
            }
        };
        fields.insert(field.name.clone(), value);
    }
    Ok(external_instance(&name.to_string(), fields))
}

fn decode_sqlite_class(
    row: &SqliteRow,
    name: &baml_type::TypeName,
    ctx: &DecodeContext,
) -> Result<BexExternalValue, SqlError> {
    let definition = ctx
        .classes
        .get(name)
        .ok_or_else(|| SqlError::decode(format!("unknown BAML class {name}"), None))?;
    let mut fields = IndexMap::new();
    for field in &definition.fields {
        let serialized = field.alias.as_deref().unwrap_or(&field.name);
        let value = match matching_column(row, serialized)? {
            Some(index) => sqlite_decode_raw(
                row.try_get_raw(index).map_err(|_| {
                    SqlError::decode("could not read result column", Some(serialized.into()))
                })?,
                &field.field_type,
                ctx,
                serialized,
            )?,
            None if nullable_target(&field.field_type).1 => BexExternalValue::Null,
            None => {
                return Err(SqlError::decode(
                    format!("required result column {serialized:?} is missing"),
                    Some(serialized.to_string()),
                ));
            }
        };
        fields.insert(field.name.clone(), value);
    }
    Ok(external_instance(&name.to_string(), fields))
}

fn row_limit(value: Option<i64>) -> Result<Option<usize>, SqlError> {
    value
        .map(|value| {
            usize::try_from(value)
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| SqlError::new("Database", "row limit must be positive"))
        })
        .transpose()
}

fn command_result(rows: u64) -> Result<owned::sql::CommandResult, SqlError> {
    Ok(owned::sql::CommandResult {
        rows_affected: i64::try_from(rows)
            .map_err(|_| SqlError::new("Database", "affected-row count exceeds BAML int range"))?,
    })
}

#[derive(Clone, Copy)]
enum SqlDialect {
    Postgres,
    Sqlite,
}

fn validate_single_statement(sql: &str, dialect: SqlDialect) -> Result<(), SqlError> {
    let bytes = sql.as_bytes();
    let mut index = 0;
    let mut saw_statement = false;
    while index < bytes.len() {
        match bytes[index] {
            byte if byte.is_ascii_whitespace() => index += 1,
            b'\'' => {
                saw_statement = true;
                index = skip_quoted(bytes, index + 1, b'\'', is_escape_string(bytes, index));
            }
            b'"' => {
                saw_statement = true;
                index = skip_quoted(bytes, index + 1, b'"', false);
            }
            b'`' => {
                saw_statement = true;
                index = skip_quoted(bytes, index + 1, b'`', false);
            }
            b'[' => {
                saw_statement = true;
                index = skip_bracket_identifier(bytes, index + 1);
            }
            b'-' if bytes.get(index + 1) == Some(&b'-') => {
                index = skip_line_comment(bytes, index + 2);
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index =
                    skip_block_comment(bytes, index + 2, matches!(dialect, SqlDialect::Postgres));
            }
            b'$' if matches!(dialect, SqlDialect::Postgres)
                && dollar_delimiter(bytes, index).is_some() =>
            {
                saw_statement = true;
                let delimiter = dollar_delimiter(bytes, index).expect("checked above");
                index = skip_dollar_quote(bytes, index + delimiter.len(), delimiter);
            }
            b';' => {
                if !saw_statement || !sql_tail_is_empty(bytes, index + 1, dialect) {
                    return Err(SqlError::new(
                        "Database",
                        "a SQL Statement must contain exactly one driver statement",
                    ));
                }
                return Ok(());
            }
            _ => {
                saw_statement = true;
                index += 1;
            }
        }
    }
    if saw_statement {
        Ok(())
    } else {
        Err(SqlError::new(
            "Database",
            "a SQL Statement must contain exactly one driver statement",
        ))
    }
}

fn is_escape_string(bytes: &[u8], quote: usize) -> bool {
    quote > 0
        && matches!(bytes[quote - 1], b'e' | b'E')
        && (quote == 1 || !bytes[quote - 2].is_ascii_alphanumeric())
}

fn skip_quoted(bytes: &[u8], mut index: usize, quote: u8, backslash_escapes: bool) -> usize {
    while index < bytes.len() {
        if backslash_escapes && bytes[index] == b'\\' {
            index = (index + 2).min(bytes.len());
        } else if bytes[index] == quote {
            if bytes.get(index + 1) == Some(&quote) {
                index += 2;
            } else {
                return index + 1;
            }
        } else {
            index += 1;
        }
    }
    index
}

fn skip_bracket_identifier(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() {
        if bytes[index] == b']' {
            if bytes.get(index + 1) == Some(&b']') {
                index += 2;
            } else {
                return index + 1;
            }
        } else {
            index += 1;
        }
    }
    index
}

fn skip_line_comment(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && !matches!(bytes[index], b'\n' | b'\r') {
        index += 1;
    }
    index
}

fn skip_block_comment(bytes: &[u8], mut index: usize, nested: bool) -> usize {
    let mut depth = 1_u32;
    while index < bytes.len() {
        if nested && bytes.get(index..index + 2) == Some(b"/*") {
            depth += 1;
            index += 2;
        } else if bytes.get(index..index + 2) == Some(b"*/") {
            depth -= 1;
            index += 2;
            if depth == 0 {
                break;
            }
        } else {
            index += 1;
        }
    }
    index
}

fn dollar_delimiter(bytes: &[u8], start: usize) -> Option<&[u8]> {
    if bytes.get(start) != Some(&b'$') {
        return None;
    }
    let mut end = start + 1;
    if bytes.get(end) == Some(&b'$') {
        return Some(&bytes[start..=end]);
    }
    if !bytes
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
    {
        return None;
    }
    end += 1;
    while bytes
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        end += 1;
    }
    (bytes.get(end) == Some(&b'$')).then_some(&bytes[start..=end])
}

fn skip_dollar_quote(bytes: &[u8], mut index: usize, delimiter: &[u8]) -> usize {
    while index + delimiter.len() <= bytes.len() {
        if &bytes[index..index + delimiter.len()] == delimiter {
            return index + delimiter.len();
        }
        index += 1;
    }
    bytes.len()
}

fn sql_tail_is_empty(bytes: &[u8], mut index: usize, dialect: SqlDialect) -> bool {
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
        } else if bytes.get(index..index + 2) == Some(b"--") {
            index = skip_line_comment(bytes, index + 2);
        } else if bytes.get(index..index + 2) == Some(b"/*") {
            index = skip_block_comment(bytes, index + 2, matches!(dialect, SqlDialect::Postgres));
        } else {
            return false;
        }
    }
    true
}

async fn execute_database(
    database: Arc<DatabaseHandle>,
    statement: Arc<SqlStatement>,
) -> Result<owned::sql::CommandResult, SqlError> {
    if database.closed.load(Ordering::Acquire) {
        return Err(SqlError::closed("database"));
    }
    match &database.pool {
        DatabasePool::Postgres(pool) => {
            let sql = statement.render_postgres();
            let arguments = pg_arguments(&statement.values)?;
            validate_single_statement(&sql, SqlDialect::Postgres)?;
            let describe = pool
                .describe(&sql)
                .await
                .map_err(|error| map_sqlx_error(error, false, false))?;
            if !describe.columns().is_empty() {
                return Err(SqlError::new(
                    "Database",
                    "execute cannot be used with a statement that returns columns",
                ));
            }
            let result = sqlx::query_with(&sql, arguments)
                .execute(pool)
                .await
                .map_err(|error| map_sqlx_error(error, false, false))?;
            command_result(result.rows_affected())
        }
        DatabasePool::Sqlite(pool) => {
            let sql = statement.render_sqlite();
            let arguments = sqlite_arguments(&statement.values)?;
            validate_single_statement(&sql, SqlDialect::Sqlite)?;
            let describe = pool
                .describe(&sql)
                .await
                .map_err(|error| map_sqlx_error(error, false, false))?;
            if !describe.columns().is_empty() {
                return Err(SqlError::new(
                    "Database",
                    "execute cannot be used with a statement that returns columns",
                ));
            }
            let result = sqlx::query_with(&sql, arguments)
                .execute(pool)
                .await
                .map_err(|error| map_sqlx_error(error, false, false))?;
            command_result(result.rows_affected())
        }
    }
}

async fn query_database(
    database: Arc<DatabaseHandle>,
    statement: Arc<SqlStatement>,
    limit: Option<usize>,
    ty: RuntimeTy,
    ctx: DecodeContext,
) -> Result<Vec<BexExternalValue>, SqlError> {
    if database.closed.load(Ordering::Acquire) {
        return Err(SqlError::closed("database"));
    }
    match &database.pool {
        DatabasePool::Postgres(pool) => {
            let sql = statement.render_postgres();
            let arguments = pg_arguments(&statement.values)?;
            validate_single_statement(&sql, SqlDialect::Postgres)?;
            let mut rows = sqlx::query_with(&sql, arguments).fetch(pool);
            let mut values = Vec::new();
            while limit.is_none_or(|limit| values.len() < limit) {
                let Some(row) = rows
                    .try_next()
                    .await
                    .map_err(|error| map_sqlx_error(error, false, false))?
                else {
                    break;
                };
                values.push(decode_pg_row(&row, &ty, &ctx)?);
            }
            Ok(values)
        }
        DatabasePool::Sqlite(pool) => {
            let sql = statement.render_sqlite();
            let arguments = sqlite_arguments(&statement.values)?;
            validate_single_statement(&sql, SqlDialect::Sqlite)?;
            let mut rows = sqlx::query_with(&sql, arguments).fetch(pool);
            let mut values = Vec::new();
            while limit.is_none_or(|limit| values.len() < limit) {
                let Some(row) = rows
                    .try_next()
                    .await
                    .map_err(|error| map_sqlx_error(error, false, false))?
                else {
                    break;
                };
                values.push(decode_sqlite_row(&row, &ty, &ctx)?);
            }
            Ok(values)
        }
    }
}

async fn execute_transaction(
    transaction: Arc<TransactionHandle>,
    statement: Arc<SqlStatement>,
) -> Result<owned::sql::CommandResult, SqlError> {
    let mut guard = transaction.connection.lock().await;
    let connection = guard
        .as_mut()
        .ok_or_else(|| SqlError::closed("transaction"))?;
    match connection {
        TransactionConnection::Postgres(connection) => {
            let sql = statement.render_postgres();
            let arguments = pg_arguments(&statement.values)?;
            validate_single_statement(&sql, SqlDialect::Postgres)?;
            let describe = (&mut **connection)
                .describe(&sql)
                .await
                .map_err(|error| map_sqlx_error(error, false, false))?;
            if !describe.columns().is_empty() {
                return Err(SqlError::new(
                    "Database",
                    "execute cannot be used with a statement that returns columns",
                ));
            }
            let result = sqlx::query_with(&sql, arguments)
                .execute(&mut **connection)
                .await
                .map_err(|error| map_sqlx_error(error, false, false))?;
            command_result(result.rows_affected())
        }
        TransactionConnection::Sqlite(connection) => {
            let sql = statement.render_sqlite();
            let arguments = sqlite_arguments(&statement.values)?;
            validate_single_statement(&sql, SqlDialect::Sqlite)?;
            let describe = (&mut **connection)
                .describe(&sql)
                .await
                .map_err(|error| map_sqlx_error(error, false, false))?;
            if !describe.columns().is_empty() {
                return Err(SqlError::new(
                    "Database",
                    "execute cannot be used with a statement that returns columns",
                ));
            }
            let result = sqlx::query_with(&sql, arguments)
                .execute(&mut **connection)
                .await
                .map_err(|error| map_sqlx_error(error, false, false))?;
            command_result(result.rows_affected())
        }
    }
}

async fn query_transaction(
    transaction: Arc<TransactionHandle>,
    statement: Arc<SqlStatement>,
    limit: Option<usize>,
    ty: RuntimeTy,
    ctx: DecodeContext,
) -> Result<Vec<BexExternalValue>, SqlError> {
    let mut guard = transaction.connection.lock().await;
    let connection = guard
        .as_mut()
        .ok_or_else(|| SqlError::closed("transaction"))?;
    match connection {
        TransactionConnection::Postgres(connection) => {
            let sql = statement.render_postgres();
            let arguments = pg_arguments(&statement.values)?;
            validate_single_statement(&sql, SqlDialect::Postgres)?;
            let mut rows = sqlx::query_with(&sql, arguments).fetch(&mut **connection);
            let mut values = Vec::new();
            while limit.is_none_or(|limit| values.len() < limit) {
                let Some(row) = rows
                    .try_next()
                    .await
                    .map_err(|error| map_sqlx_error(error, false, false))?
                else {
                    break;
                };
                values.push(decode_pg_row(&row, &ty, &ctx)?);
            }
            Ok(values)
        }
        TransactionConnection::Sqlite(connection) => {
            let sql = statement.render_sqlite();
            let arguments = sqlite_arguments(&statement.values)?;
            validate_single_statement(&sql, SqlDialect::Sqlite)?;
            let mut rows = sqlx::query_with(&sql, arguments).fetch(&mut **connection);
            let mut values = Vec::new();
            while limit.is_none_or(|limit| values.len() < limit) {
                let Some(row) = rows
                    .try_next()
                    .await
                    .map_err(|error| map_sqlx_error(error, false, false))?
                else {
                    break;
                };
                values.push(decode_sqlite_row(&row, &ty, &ctx)?);
            }
            Ok(values)
        }
    }
}

fn scalar_result(mut values: Vec<BexExternalValue>) -> Result<BexExternalValue, SqlError> {
    match values.len() {
        0 => Err(SqlError::new("NotFound", "query returned no rows")),
        1 => Ok(values.pop().expect("length checked")),
        _ => Err(SqlError::new(
            "TooManyRows",
            "query returned more than one row",
        )),
    }
}

fn transaction_value(handle: Arc<TransactionHandle>) -> owned::sql::Transaction {
    let handle: Arc<dyn std::any::Any + Send + Sync> = handle;
    owned::sql::Transaction { _handle: handle }
}

async fn begin_transaction(
    database: Arc<DatabaseHandle>,
    options: owned::sql::TransactionOptions,
) -> Result<owned::sql::Transaction, SqlError> {
    if database.closed.load(Ordering::Acquire) {
        return Err(SqlError::closed("database"));
    }
    let isolation = options.isolation.as_ref().and_then(enum_variant);
    match &database.pool {
        DatabasePool::Postgres(pool) => {
            let connection = pool
                .acquire()
                .await
                .map_err(|error| map_sqlx_error(error, false, false))?;
            let mut connection =
                DiscardOnDropConnection(Some(TransactionConnection::Postgres(connection)));
            let isolation = match isolation {
                None => "",
                Some("ReadCommitted") => " ISOLATION LEVEL READ COMMITTED",
                Some("RepeatableRead") => " ISOLATION LEVEL REPEATABLE READ",
                Some("Serializable") => " ISOLATION LEVEL SERIALIZABLE",
                Some(_) => return Err(SqlError::unsupported("unsupported PostgreSQL isolation")),
            };
            let read_only = if options.read_only == Some(true) {
                " READ ONLY"
            } else {
                ""
            };
            let Some(TransactionConnection::Postgres(provider_connection)) = connection.0.as_mut()
            else {
                unreachable!("PostgreSQL transaction guard has PostgreSQL connection")
            };
            sqlx::query(&format!("BEGIN{isolation}{read_only}"))
                .execute(&mut **provider_connection)
                .await
                .map_err(|error| map_sqlx_error(error, false, false))?;
            Ok(transaction_value(Arc::new(TransactionHandle {
                connection: Mutex::new(connection.0.take()),
                default_timeout: database.default_timeout,
                sqlite_query_only: false,
            })))
        }
        DatabasePool::Sqlite(pool) => {
            if matches!(isolation, Some("ReadCommitted" | "RepeatableRead")) {
                return Err(SqlError::unsupported(
                    "SQLite supports only Serializable isolation",
                ));
            }
            let connection = pool
                .acquire()
                .await
                .map_err(|error| map_sqlx_error(error, false, false))?;
            let mut connection =
                DiscardOnDropConnection(Some(TransactionConnection::Sqlite(connection)));
            let Some(TransactionConnection::Sqlite(provider_connection)) = connection.0.as_mut()
            else {
                unreachable!("SQLite transaction guard has SQLite connection")
            };
            let query_only = options.read_only == Some(true);
            if query_only {
                sqlx::query("PRAGMA query_only = ON")
                    .execute(&mut **provider_connection)
                    .await
                    .map_err(|error| map_sqlx_error(error, false, false))?;
            }
            if let Err(error) = sqlx::query("BEGIN DEFERRED")
                .execute(&mut **provider_connection)
                .await
            {
                if query_only {
                    let _ = sqlx::query("PRAGMA query_only = OFF")
                        .execute(&mut **provider_connection)
                        .await;
                }
                return Err(map_sqlx_error(error, false, false));
            }
            Ok(transaction_value(Arc::new(TransactionHandle {
                connection: Mutex::new(connection.0.take()),
                default_timeout: database.default_timeout,
                sqlite_query_only: query_only,
            })))
        }
    }
}

async fn finalize_transaction(
    transaction: Arc<TransactionHandle>,
    commit: bool,
) -> Result<(), SqlError> {
    let mut guard = transaction.connection.lock().await;
    let Some(connection) = guard.take() else {
        return if commit {
            Err(SqlError::closed("transaction"))
        } else {
            Ok(())
        };
    };
    let mut connection = DiscardOnDropConnection(Some(connection));
    let mut failure = None;
    match connection
        .0
        .as_mut()
        .expect("transaction finalization guard has a connection")
    {
        TransactionConnection::Postgres(connection) => {
            if let Err(error) = sqlx::query(if commit { "COMMIT" } else { "ROLLBACK" })
                .execute(&mut **connection)
                .await
            {
                if !commit {
                    connection.close_on_drop();
                }
                failure = Some((map_sqlx_error(error, false, false), commit));
            }
        }
        TransactionConnection::Sqlite(connection) => {
            if let Err(error) = sqlx::query(if commit { "COMMIT" } else { "ROLLBACK" })
                .execute(&mut **connection)
                .await
            {
                if !commit {
                    connection.close_on_drop();
                }
                failure = Some((map_sqlx_error(error, false, false), commit));
            } else if transaction.sqlite_query_only
                && let Err(error) = sqlx::query("PRAGMA query_only = OFF")
                    .execute(&mut **connection)
                    .await
            {
                connection.close_on_drop();
                failure = Some((map_sqlx_error(error, false, false), false));
            }
        }
    }
    if let Some((error, restore_for_rollback)) = failure {
        if restore_for_rollback {
            *guard = connection.0.take();
        }
        return Err(error);
    }
    drop(connection.0.take());
    Ok(())
}

impl io::IoClassSqlDatabase for NativeSysOps {
    fn _close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        database: owned::sql::Database,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        sql_output(async move {
            let database = downcast_database(&database)?;
            if database.closed.swap(true, Ordering::AcqRel) {
                return Ok(());
            }
            let timeout = database.default_timeout;
            with_timeout(timeout, async {
                match &database.pool {
                    DatabasePool::Postgres(pool) => pool.close().await,
                    DatabasePool::Sqlite(pool) => pool.close().await,
                }
                Ok(())
            })
            .await
        })
    }

    fn _execute(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        database: owned::sql::Database,
        statement: owned::sql::Statement,
        timeout_nanos: Option<Arc<BigInt>>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::sql::CommandResult> {
        sql_output(async move {
            let database = downcast_database(&database)?;
            let statement = downcast_statement(&statement)?;
            let timeout = operation_timeout(timeout_nanos, database.default_timeout)?;
            with_timeout(timeout, execute_database(database, statement)).await
        })
    }

    fn _query(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        database: owned::sql::Database,
        statement: owned::sql::Statement,
        timeout_nanos: Option<Arc<BigInt>>,
        requested_limit: Option<i64>,
        type_arg_0: RuntimeTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<BexExternalValue>> {
        let decode = DecodeContext::new(ctx);
        sql_output(async move {
            let database = downcast_database(&database)?;
            let statement = downcast_statement(&statement)?;
            let timeout = operation_timeout(timeout_nanos, database.default_timeout)?;
            let limit = row_limit(requested_limit)?;
            with_timeout(
                timeout,
                query_database(database, statement, limit, type_arg_0, decode),
            )
            .await
        })
    }

    fn _scalar(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        database: owned::sql::Database,
        statement: owned::sql::Statement,
        timeout_nanos: Option<Arc<BigInt>>,
        requested_limit: i64,
        type_arg_0: RuntimeTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let decode = DecodeContext::new(ctx);
        sql_output(async move {
            let database = downcast_database(&database)?;
            let statement = downcast_statement(&statement)?;
            let timeout = operation_timeout(timeout_nanos, database.default_timeout)?;
            let limit = row_limit(Some(requested_limit))?;
            let values = with_timeout(
                timeout,
                query_database(database, statement, limit, type_arg_0, decode),
            )
            .await?;
            scalar_result(values)
        })
    }

    fn _begin(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        database: owned::sql::Database,
        options: Option<owned::sql::TransactionOptions>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::sql::Transaction> {
        sql_output(async move {
            let database = downcast_database(&database)?;
            let timeout = database.default_timeout;
            with_timeout(
                timeout,
                begin_transaction(database, options.unwrap_or_default()),
            )
            .await
        })
    }
}

impl io::IoClassSqlTransaction for NativeSysOps {
    fn _execute(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        transaction: owned::sql::Transaction,
        statement: owned::sql::Statement,
        timeout_nanos: Option<Arc<BigInt>>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::sql::CommandResult> {
        sql_output(async move {
            let transaction = downcast_transaction(&transaction)?;
            let statement = downcast_statement(&statement)?;
            let timeout = operation_timeout(timeout_nanos, transaction.default_timeout)?;
            with_timeout(timeout, execute_transaction(transaction, statement)).await
        })
    }

    fn _query(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        transaction: owned::sql::Transaction,
        statement: owned::sql::Statement,
        timeout_nanos: Option<Arc<BigInt>>,
        requested_limit: Option<i64>,
        type_arg_0: RuntimeTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<BexExternalValue>> {
        let decode = DecodeContext::new(ctx);
        sql_output(async move {
            let transaction = downcast_transaction(&transaction)?;
            let statement = downcast_statement(&statement)?;
            let timeout = operation_timeout(timeout_nanos, transaction.default_timeout)?;
            let limit = row_limit(requested_limit)?;
            with_timeout(
                timeout,
                query_transaction(transaction, statement, limit, type_arg_0, decode),
            )
            .await
        })
    }

    fn _scalar(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        transaction: owned::sql::Transaction,
        statement: owned::sql::Statement,
        timeout_nanos: Option<Arc<BigInt>>,
        requested_limit: i64,
        type_arg_0: RuntimeTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let decode = DecodeContext::new(ctx);
        sql_output(async move {
            let transaction = downcast_transaction(&transaction)?;
            let statement = downcast_statement(&statement)?;
            let timeout = operation_timeout(timeout_nanos, transaction.default_timeout)?;
            let limit = row_limit(Some(requested_limit))?;
            let values = with_timeout(
                timeout,
                query_transaction(transaction, statement, limit, type_arg_0, decode),
            )
            .await?;
            scalar_result(values)
        })
    }

    fn _commit(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        transaction: owned::sql::Transaction,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        sql_output(async move {
            let transaction = downcast_transaction(&transaction)?;
            let timeout = transaction.default_timeout;
            with_timeout(timeout, finalize_transaction(transaction, true)).await
        })
    }

    fn _rollback_if_open(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        transaction: owned::sql::Transaction,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        sql_output(async move {
            let transaction = downcast_transaction(&transaction)?;
            let timeout = transaction.default_timeout;
            with_timeout(timeout, finalize_transaction(transaction, false)).await
        })
    }
}

impl io::IoNamespaceSql for NativeSysOps {
    fn _connect(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        url: String,
        options: Option<owned::sql::ConnectOptions>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::sql::Database> {
        sql_output(async move {
            let options = options.unwrap_or_default();
            let handle = if url.starts_with("postgres://") || url.starts_with("postgresql://") {
                connect_postgres(
                    url,
                    owned::sql_postgres::PostgresOptions {
                        max_connections: options.max_connections,
                        connect_timeout: options.connect_timeout,
                        query_timeout: options.query_timeout,
                        validate: options.validate,
                        ..Default::default()
                    },
                )
                .await?
            } else if url.starts_with("sqlite:") || url.starts_with("file:") || url == ":memory:" {
                connect_sqlite(
                    url.clone(),
                    owned::sql_sqlite::SqliteOptions {
                        max_connections: options.max_connections,
                        query_timeout: options.query_timeout,
                        validate: options.validate,
                        ..Default::default()
                    },
                    url == ":memory:",
                )
                .await?
            } else {
                return Err(SqlError::connection(
                    "SQL URLs must use an explicit supported provider scheme",
                ));
            };
            Ok(database_value(handle))
        })
    }

    fn _connect_postgres(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        url: String,
        options: Option<owned::sql_postgres::PostgresOptions>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        sql_output(async move {
            connect_postgres(url, options.unwrap_or_default())
                .await
                .map(database_external)
        })
    }

    fn _open_sqlite(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        options: Option<owned::sql_sqlite::SqliteOptions>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        sql_output(async move {
            connect_sqlite(path, options.unwrap_or_default(), false)
                .await
                .map(database_external)
        })
    }

    fn _memory_sqlite(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        options: Option<owned::sql_sqlite::SqliteOptions>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        sql_output(async move {
            connect_sqlite(String::new(), options.unwrap_or_default(), true)
                .await
                .map(database_external)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_statement_validation_ignores_quoted_and_commented_semicolons() {
        for sql in [
            "SELECT ';'",
            "SELECT \";\"",
            "SELECT `;`",
            "SELECT [;]",
            "SELECT 1 -- ;\n",
            "SELECT 1; /* trailing comment */ -- still trailing\n",
        ] {
            for dialect in [SqlDialect::Postgres, SqlDialect::Sqlite] {
                assert!(validate_single_statement(sql, dialect).is_ok(), "{sql}");
            }
        }
        for sql in [
            "SELECT $$;$$",
            "SELECT $tag$;$tag$",
            "SELECT 1 /* ; /* nested ; */ ; */",
        ] {
            assert!(
                validate_single_statement(sql, SqlDialect::Postgres).is_ok(),
                "{sql}"
            );
            assert!(
                validate_single_statement(sql, SqlDialect::Sqlite).is_err(),
                "{sql}"
            );
        }
    }

    #[test]
    fn multiple_driver_statements_are_rejected() {
        for sql in [
            "",
            "  -- no statement\n /* still empty */ ",
            ";",
            "SELECT 1; SELECT 2",
            "SELECT 1;;",
            "SELECT 1; -- x\n SELECT 2",
        ] {
            for dialect in [SqlDialect::Postgres, SqlDialect::Sqlite] {
                let error = validate_single_statement(sql, dialect).unwrap_err();
                assert_eq!(error.kind, "Database");
            }
        }
    }

    #[test]
    fn explicit_sqlite_mode_replaces_uri_mode() {
        let (url, mode) = sqlite_mode_url(
            "sqlite:test.db?mode=ro&cache=private",
            Some("ReadWriteCreate"),
        )
        .unwrap();
        assert_eq!(mode.as_deref(), Some("rwc"));
        assert!(url.contains("mode=rwc"));
        assert!(!url.contains("mode=ro"));
        assert!(url.contains("cache=private"));

        assert!(sqlite_mode_url("sqlite:test.db?mode=ro&mode=rw", None).is_err());
        assert!(sqlite_mode_url("sqlite:test.db?cache=private&cache=shared", None).is_err());
    }

    #[test]
    fn ssl_requirement_cannot_be_weakened() {
        assert!(ssl_mode_rank("verify-full") > ssl_mode_rank("require"));
        assert!(ssl_mode_rank("require") > ssl_mode_rank("prefer"));
    }

    #[test]
    fn duration_options_support_more_than_u64_nanoseconds() {
        let nanos = BigInt::from(u64::MAX) + BigInt::from(1_u8);
        let duration = duration_bigint(&nanos).unwrap();
        assert_eq!(
            u128::from(duration.as_secs()) * 1_000_000_000 + u128::from(duration.subsec_nanos()),
            u128::from(u64::MAX) + 1
        );
    }
}
