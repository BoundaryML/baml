//! `obs-bench validate` (design §10.3, C8): walk a `.baml`-shaped root and
//! verify every v2 observability artifact — framing, CRCs, and the
//! committed-prefix recovery contract. Torn tails and truncated meta are
//! VALID states (crash evidence, readable prefix); undecodable committed
//! bytes are not.

use std::path::{Path, PathBuf};

use bex_events::prof::cct::meta;
use bex_events::prof::cct::raw;
use bex_events::prof::cct::segment::{self, ScanEnd};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Finding {
    pub path: String,
    /// `bamlseg` | `bamlmeta` | `bamlcct` | `bamlraw` | `bamldict`.
    pub kind: &'static str,
    /// `sealed` | `active` | `torn` | `ok` | `truncated` | `invalid`.
    pub status: &'static str,
    pub detail: String,
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub files: usize,
    pub invalid: usize,
    pub torn: usize,
    /// Legacy `profiles/` files counted but not validated here (they have
    /// their own gate suite).
    pub legacy_files: usize,
}

impl Report {
    fn push(&mut self, path: &Path, kind: &'static str, status: &'static str, detail: String) {
        self.files += 1;
        if status == "invalid" {
            self.invalid += 1;
        }
        if status == "torn" || status == "truncated" {
            self.torn += 1;
        }
        self.findings.push(Finding {
            path: path.display().to_string(),
            kind,
            status,
            detail,
        });
    }

    #[must_use]
    pub fn render(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        for f in &self.findings {
            let _ = writeln!(out, "{:8} {:8} {} ({})", f.status, f.kind, f.path, f.detail);
        }
        let _ = writeln!(
            out,
            "validate: {} files, {} invalid, {} torn/truncated, {} legacy (uninspected)",
            self.files, self.invalid, self.torn, self.legacy_files
        );
        out
    }
}

/// Validate one `.baml`-shaped root (the dir holding `sessions/`,
/// `history/`, `dict/`, `profiles/`).
#[must_use]
pub fn validate_root(root: &Path) -> Report {
    let mut report = Report::default();

    for dict in files_with_ext(&root.join("dict"), "bamldict") {
        match std::fs::read(&dict).map(|b| bex_events::dict::read_dict(&b)) {
            Ok(Ok(d)) => report.push(
                &dict,
                "bamldict",
                "ok",
                format!(
                    "{} function rows",
                    d.functions.as_ref().map_or(0, |s| s.functions.len())
                ),
            ),
            Ok(Err(err)) => report.push(&dict, "bamldict", "invalid", err.to_string()),
            Err(err) => report.push(&dict, "bamldict", "invalid", err.to_string()),
        }
    }

    let sessions_root = root.join("sessions");
    if let Ok(entries) = std::fs::read_dir(&sessions_root) {
        for session in entries.filter_map(Result::ok).map(|e| e.path()) {
            if !session.is_dir() {
                continue;
            }
            validate_meta(&session.join("session.bamlmeta"), 1, &mut report);
            for seg in files_with_ext(&session.join("cct"), "bamlseg") {
                validate_segment(&seg, false, &mut report);
            }
            for raw_file in files_with_ext(&session.join("raw"), "bamlprof") {
                validate_raw(&raw_file, &mut report);
            }
            for dump in files_with_ext(&session.join("flight"), "bamlprof") {
                validate_flight_dump(&dump, &mut report);
            }
        }
    }

    let history_root = root.join("history");
    if let Ok(entries) = std::fs::read_dir(&history_root) {
        for boundary in entries.filter_map(Result::ok).map(|e| e.path()) {
            if !boundary.is_dir() || boundary.file_name().is_some_and(|n| n == "_unbound") {
                continue;
            }
            let meta_path = boundary.join("boundary.bamlmeta");
            if meta_path.exists() {
                validate_meta(&meta_path, 16, &mut report);
            }
            let snapshot = boundary.join("cct.bamlcct");
            if snapshot.exists() {
                // tmp+rename write: a visible cct.bamlcct is sealed or bust.
                validate_segment(&snapshot, true, &mut report);
            }
        }
    }

    report.legacy_files = files_with_ext(&root.join("profiles"), "bamlprof").len();
    report
}

fn validate_segment(path: &Path, must_be_sealed: bool, report: &mut Report) {
    let kind = if path.extension().is_some_and(|e| e == "bamlcct") {
        "bamlcct"
    } else {
        "bamlseg"
    };
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(err) => return report.push(path, kind, "invalid", err.to_string()),
    };
    match segment::scan_segment(&bytes) {
        Ok(contents) => {
            let detail = format!("{} committed blocks", contents.blocks.len());
            match contents.end {
                ScanEnd::Sealed => report.push(path, kind, "sealed", detail),
                ScanEnd::ActiveEnd if must_be_sealed => report.push(
                    path,
                    kind,
                    "invalid",
                    format!("{detail}; snapshot must be sealed (tmp+rename contract)"),
                ),
                ScanEnd::ActiveEnd => report.push(path, kind, "active", detail),
                ScanEnd::Torn { offset } if must_be_sealed => report.push(
                    path,
                    kind,
                    "invalid",
                    format!("{detail}; torn at {offset} in an always-sealed snapshot"),
                ),
                ScanEnd::Torn { offset } => {
                    report.push(path, kind, "torn", format!("{detail}; torn at {offset}"));
                }
            }
        }
        Err(err) => report.push(path, kind, "invalid", format!("{err:?}")),
    }
}

fn validate_meta(path: &Path, expect_first_kind: u8, report: &mut Report) {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(err) => return report.push(path, "bamlmeta", "invalid", err.to_string()),
    };
    match meta::read_meta(&bytes) {
        Ok(contents) => {
            let first = contents.records.first().map(meta::MetaRecord::kind);
            if first != Some(expect_first_kind) {
                return report.push(
                    path,
                    "bamlmeta",
                    "invalid",
                    format!("first record kind {first:?}, expected {expect_first_kind}"),
                );
            }
            let detail = format!(
                "{} records, {} unknown",
                contents.records.len(),
                contents.unknown_records
            );
            if contents.truncated {
                report.push(path, "bamlmeta", "truncated", detail);
            } else {
                report.push(path, "bamlmeta", "ok", detail);
            }
        }
        Err(err) => report.push(path, "bamlmeta", "invalid", format!("{err:?}")),
    }
}

/// §5.9 flight dumps reuse the exact legacy `.bamlprof` framing — the
/// standard reader must parse them; truncation is crash evidence.
fn validate_flight_dump(path: &Path, report: &mut Report) {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(err) => return report.push(path, "flight", "invalid", err.to_string()),
    };
    match bex_events::prof::read::read_bamlprof_from_bytes(&bytes) {
        Ok(contents) => {
            let detail = format!("{} events", contents.events.len());
            if contents.truncated {
                report.push(path, "flight", "torn", detail);
            } else {
                report.push(path, "flight", "ok", detail);
            }
        }
        Err(err) => report.push(path, "flight", "invalid", format!("{err:?}")),
    }
}

fn validate_raw(path: &Path, report: &mut Report) {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(err) => return report.push(path, "bamlraw", "invalid", err.to_string()),
    };
    match raw::read_raw_file(&bytes) {
        Ok(parsed) => {
            let mut records: u64 = 0;
            for range in &parsed.ranges {
                for rec in bex_events::prof::record::iter(range) {
                    match rec {
                        Ok(_) => records += 1,
                        Err(err) => {
                            return report.push(
                                path,
                                "bamlraw",
                                "invalid",
                                format!("undecodable committed range: {err:?}"),
                            );
                        }
                    }
                }
            }
            let detail = format!("{} ranges, {records} records", parsed.ranges.len());
            if parsed.torn_bytes > 0 {
                report.push(
                    path,
                    "bamlraw",
                    "torn",
                    format!("{detail}, {} torn bytes", parsed.torn_bytes),
                );
            } else {
                report.push(path, "bamlraw", "ok", detail);
            }
        }
        Err(err) => report.push(path, "bamlraw", "invalid", err.to_string()),
    }
}

fn files_with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == ext))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}
