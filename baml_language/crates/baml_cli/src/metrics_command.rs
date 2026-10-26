//! `baml __metrics`: code complexity metrics as one JSON document.
//!
//! Internal and unstable: hidden from help, and the output shape may change
//! without notice. It exists for skill evals and for finding hard-to-read
//! code, not as a supported user feature.
#![allow(
    clippy::print_stdout,
    reason = "the JSON document on stdout is the command's result"
)]

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use baml_db::{ProjectDatabase, SourceFile, SourceRootKind, line_index::LineIndex};
use baml_ide::metrics::{
    Abc, Calls, FnKind, FunctionMetrics, Halstead, Loc, Maintainability, TypeKind, TypeMetrics,
    project_metrics,
};
use clap::Args;
use serde::Serialize;

use crate::project_session::{CacheUse, ProjectSession};

/// Print complexity metrics for every function and type as JSON (unstable).
#[derive(Args, Debug)]
pub(crate) struct MetricsArgs {
    /// Only report functions and types in files under these paths. Calls
    /// from the rest of the project still count toward fan-in.
    pub(crate) paths: Vec<PathBuf>,

    /// Measure the standard library instead of a project.
    #[arg(long, conflicts_with = "paths")]
    pub(crate) stdlib: bool,

    /// Set from the global `--project`.
    #[arg(skip)]
    pub(crate) from: Option<PathBuf>,
}

impl MetricsArgs {
    pub(crate) fn run(&self) -> Result<crate::ExitCode> {
        let report = if self.stdlib {
            let mut db = ProjectDatabase::new();
            db.ensure_stdlib_sources();
            let files: Vec<SourceFile> = db
                .source_roots()
                .into_iter()
                .filter(|root| root.kind(&db) == SourceRootKind::Stdlib)
                .flat_map(|root| root.files(&db).clone())
                .collect();
            report(&db, &files, None, &[])
        } else {
            let session = ProjectSession::open(self.from.as_deref(), CacheUse::Off)?;
            if session.is_empty() {
                crate::reporter::print_error(format_args!(
                    "no .baml files found in {}",
                    session.root().display()
                ));
                return Ok(crate::ExitCode::Other);
            }
            let only = self
                .paths
                .iter()
                .map(|path| {
                    std::path::absolute(path)
                        .with_context(|| format!("could not resolve path: {}", path.display()))
                })
                .collect::<Result<Vec<_>>>()?;
            let files = session.package.files(&session.db).clone();
            report(&session.db, &files, Some(session.root()), &only)
        };
        println!("{}", serde_json::to_string(&report)?);
        Ok(crate::ExitCode::Success)
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct Report {
    pub(crate) functions: Vec<FunctionReport>,
    pub(crate) types: Vec<TypeReport>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FunctionReport {
    pub(crate) name: String,
    pub(crate) kind: FnKind,
    pub(crate) file: String,
    /// One-based and inclusive.
    pub(crate) start_line: u32,
    pub(crate) end_line: u32,
    pub(crate) loc: Loc,
    pub(crate) params: u32,
    pub(crate) cyclomatic: u32,
    pub(crate) cognitive: u32,
    /// Saturates at `u64::MAX`.
    pub(crate) npath: u64,
    pub(crate) nesting: u32,
    pub(crate) exits: u32,
    pub(crate) abc: Abc,
    pub(crate) abc_magnitude: f64,
    pub(crate) halstead: HalsteadReport,
    pub(crate) maintainability: Maintainability,
    pub(crate) calls: Calls,
    pub(crate) henry_kafura: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct HalsteadReport {
    #[serde(flatten)]
    pub(crate) counts: Halstead,
    pub(crate) volume: f64,
    pub(crate) difficulty: f64,
    pub(crate) effort: f64,
    pub(crate) bugs: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct TypeReport {
    pub(crate) name: String,
    pub(crate) kind: TypeKind,
    pub(crate) file: String,
    pub(crate) fields: u32,
    pub(crate) nom: u32,
    pub(crate) wmc: u32,
    pub(crate) rfc: u32,
    pub(crate) cbo: u32,
    pub(crate) lcom4: Option<u32>,
    pub(crate) interfaces: u32,
}

/// Metrics over all of `files`, keeping only items in files under one of
/// `only` (every item when it is empty). Paths are shown relative to `root`
/// when they are under it.
pub(crate) fn report(
    db: &ProjectDatabase,
    files: &[SourceFile],
    root: Option<&Path>,
    only: &[PathBuf],
) -> Report {
    let metrics = project_metrics(db, files);
    let keep =
        |file: SourceFile| only.is_empty() || only.iter().any(|p| file.path(db).starts_with(p));
    let shown = |file: SourceFile| {
        let path = file.path(db);
        root.and_then(|root| path.strip_prefix(root).ok())
            .unwrap_or(&path)
            .display()
            .to_string()
    };
    Report {
        functions: metrics
            .functions
            .iter()
            .filter(|f| keep(f.file))
            .map(|f| function_report(db, f, shown(f.file)))
            .collect(),
        types: metrics
            .types
            .iter()
            .filter(|t| keep(t.file))
            .map(|t| type_report(t, shown(t.file)))
            .collect(),
    }
}

fn function_report(db: &ProjectDatabase, f: &FunctionMetrics, file: String) -> FunctionReport {
    let lines = LineIndex::new(f.file.text(db));
    let h = f.halstead;
    FunctionReport {
        name: f.name.to_string(),
        kind: f.kind,
        file,
        start_line: lines.line(f.span.start().into()) + 1,
        // The span's end is exclusive.
        end_line: lines.line(u32::from(f.span.end()).saturating_sub(1)) + 1,
        loc: f.loc,
        params: f.params,
        cyclomatic: f.cyclomatic,
        cognitive: f.cognitive,
        npath: f.npath,
        nesting: f.max_nesting,
        exits: f.exits,
        abc: f.abc,
        abc_magnitude: f.abc.magnitude(),
        halstead: HalsteadReport {
            counts: h,
            volume: h.volume(),
            difficulty: h.difficulty(),
            effort: h.effort(),
            bugs: h.bugs(),
        },
        maintainability: f.maintainability(),
        calls: f.calls,
        henry_kafura: f.henry_kafura(),
    }
}

fn type_report(t: &TypeMetrics, file: String) -> TypeReport {
    TypeReport {
        name: t.name.to_string(),
        kind: t.kind,
        file,
        fields: t.fields,
        nom: t.nom,
        wmc: t.wmc,
        rfc: t.rfc,
        cbo: t.cbo,
        lcom4: t.lcom4,
        interfaces: t.interfaces_implemented,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let path = dir.path().join("baml_src").join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        dir
    }

    fn report_for(dir: &Path, only: &[&str]) -> Report {
        let session = ProjectSession::open(Some(dir), CacheUse::Off).unwrap();
        let only: Vec<PathBuf> = only.iter().map(|p| session.root().join(p)).collect();
        let files = session.package.files(&session.db).clone();
        report(&session.db, &files, Some(session.root()), &only)
    }

    const MAIN: &str = "\
function pick(n: int) -> string {
    if (n > 1) {
        \"many\"
    } else {
        \"one\"
    }
}

class Counter {
    n: int,

    function bump(self) -> int {
        self.n += 1;
        pick(self.n).length()
    }
}
";

    #[test]
    fn reports_functions_with_lines_and_types() {
        let dir = project(&[("main.baml", MAIN)]);
        let report = report_for(dir.path(), &[]);

        let pick = report.functions.iter().find(|f| f.name == "pick").unwrap();
        assert_eq!(
            pick.file,
            Path::new("baml_src")
                .join("main.baml")
                .display()
                .to_string()
        );
        assert_eq!((pick.start_line, pick.end_line), (1, 7));
        assert_eq!((pick.cyclomatic, pick.cognitive, pick.npath), (2, 2, 2));
        assert_eq!(pick.calls.fan_in, 1);

        let bump = report
            .functions
            .iter()
            .find(|f| f.name == "Counter.bump")
            .unwrap();
        assert_eq!((bump.start_line, bump.end_line), (12, 15));
        // `pick` and `string.length`.
        assert_eq!(bump.calls.fan_out, 2);

        let counter = report.types.iter().find(|t| t.name == "Counter").unwrap();
        assert_eq!((counter.fields, counter.nom), (1, 1));
    }

    #[test]
    fn path_filter_keeps_fan_in_from_other_files() {
        let caller = "function twice() -> string {\n    pick(1) + pick(2)\n}\n";
        let dir = project(&[("main.baml", MAIN), ("caller.baml", caller)]);
        let report = report_for(dir.path(), &["baml_src/main.baml"]);

        assert!(
            report
                .functions
                .iter()
                .all(|f| f.file.ends_with("main.baml"))
        );
        let pick = report.functions.iter().find(|f| f.name == "pick").unwrap();
        assert_eq!(pick.calls.fan_in, 2);
    }
}
