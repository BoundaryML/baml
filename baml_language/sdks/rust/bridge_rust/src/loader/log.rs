//! Minimal stderr logger for the library loader.
//!
//! The loader runs before any engine is available, so it cannot use the
//! engine's logging pipeline. Levels follow the engine's `BAML_LOG`
//! convention (TRACE/DEBUG/INFO/WARN/ERROR/OFF, default INFO; an invalid
//! value is reported once and treated as INFO), and lines carry the same
//! `<timestamp> [BAML <LEVEL>] <message>` shape as the other bridges'
//! loaders. Level colors are applied only when stderr is a terminal.

use std::{
    io::{IsTerminal, Write},
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    /// Sentinel threshold above every real level; never used to emit.
    Off,
}

impl Level {
    fn name(self) -> &'static str {
        match self {
            Level::Trace => "TRACE",
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
            Level::Off => "OFF",
        }
    }

    fn color(self) -> &'static str {
        match self {
            Level::Trace => "\x1b[94m", // blue
            Level::Debug => "\x1b[90m", // gray
            Level::Info => "\x1b[92m",  // green
            Level::Warn => "\x1b[93m",  // yellow
            Level::Error => "\x1b[91m", // red
            Level::Off => "",
        }
    }
}

const RESET: &str = "\x1b[0m";

fn configured_level() -> Level {
    static LEVEL: OnceLock<Level> = OnceLock::new();
    *LEVEL.get_or_init(|| {
        let raw = std::env::var("BAML_LOG").unwrap_or_default();
        match raw.to_ascii_uppercase().as_str() {
            "TRACE" => Level::Trace,
            "DEBUG" => Level::Debug,
            "" | "INFO" => Level::Info,
            "WARN" | "WARNING" => Level::Warn,
            "ERROR" => Level::Error,
            "OFF" => Level::Off,
            other => {
                emit(
                    Level::Warn,
                    &format!("Invalid BAML_LOG '{other}'. Defaulting to INFO."),
                );
                Level::Info
            }
        }
    })
}

pub(crate) fn debug(msg: &str) {
    log(Level::Debug, msg);
}

pub(crate) fn info(msg: &str) {
    log(Level::Info, msg);
}

pub(crate) fn warn(msg: &str) {
    log(Level::Warn, msg);
}

fn log(level: Level, msg: &str) {
    if level < configured_level() {
        return;
    }
    emit(level, msg);
}

/// Write one line unconditionally (used both for regular logging and for
/// reporting an invalid `BAML_LOG` before the level is settled).
fn emit(level: Level, msg: &str) {
    let stderr = std::io::stderr();
    let level_str = if stderr.is_terminal() {
        format!("{}{}{RESET}", level.color(), level.name())
    } else {
        level.name().to_string()
    };
    // A failed write to stderr has no better channel to report through.
    let _ = writeln!(
        stderr.lock(),
        "{} [BAML {level_str}] {msg}",
        utc_timestamp()
    );
}

fn utc_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let (year, month, day) = civil_from_days((secs / 86_400).cast_signed());
    let rem = secs % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60,
        now.subsec_millis()
    )
}

/// Days since 1970-01-01 → (year, month, day) in the proleptic Gregorian
/// calendar (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        year,
        u32::try_from(month).unwrap_or_else(|_| unreachable!("month is in [1, 12]")),
        u32::try_from(day).unwrap_or_else(|_| unreachable!("day is in [1, 31]")),
    )
}

#[cfg(test)]
mod tests {
    use super::civil_from_days;

    #[test]
    fn civil_from_days_matches_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        // 2000-02-29 (leap day) is 11_016 days after the epoch.
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        // 2026-07-14 is 20_648 days after the epoch.
        assert_eq!(civil_from_days(20_648), (2026, 7, 14));
    }
}
