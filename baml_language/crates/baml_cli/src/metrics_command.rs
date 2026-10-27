//! `baml metrics`: code complexity metrics, broad to narrow. The default view
//! summarizes the project; `--files` gives one row per file and
//! `--functions` one row per function, worst first.
#![allow(
    clippy::print_stdout,
    reason = "the metrics table on stdout is the command's result"
)]

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use baml_db::{ProjectDatabase, SourceFile, line_index::LineIndex};
use baml_ide::metrics::{FileMetrics, FunctionMetrics, ProjectMetrics, project_metrics};
use clap::{Args, ValueEnum};

use crate::project_session::{CacheUse, ProjectSession};

/// Rows a list view shows without `--all`.
const TOP: usize = 10;

/// Print code complexity metrics (experimental).
#[derive(Args, Debug)]
#[command(after_long_help = "\
Examples:
  Summarize the project:
    baml metrics

  The files with the most complex functions:
    baml metrics --files

  Every function in one file, by nesting depth:
    baml metrics --functions --all --metrics nesting,cognitive baml_src/router.baml")]
pub(crate) struct MetricsArgs {
    /// Only report functions and files under these paths. Calls from the rest
    /// of the project still count toward fan-in.
    pub(crate) paths: Vec<PathBuf>,

    /// One row per file.
    #[arg(long, conflicts_with = "functions")]
    pub(crate) files: bool,

    /// One row per function.
    #[arg(long)]
    pub(crate) functions: bool,

    /// Metrics to show, comma-separated. Rows sort worst first by the first
    /// one. Defaults to `cognitive,nesting,npath,exits,lines`, plus
    /// `tests,test-lines` outside `--functions`.
    #[arg(long, value_delimiter = ',', value_name = "LIST")]
    pub(crate) metrics: Vec<Metric>,

    /// Show every row instead of the worst 10.
    #[arg(long)]
    pub(crate) all: bool,

    /// Set from the global `--project`.
    #[arg(skip)]
    pub(crate) from: Option<PathBuf>,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Metric {
    /// Lines holding code. For a file or the project, lines outside test
    /// blocks.
    Lines,
    /// `test` blocks. Files and the project only.
    Tests,
    /// Lines holding code inside `test` and `testset` blocks. Files and the
    /// project only.
    TestLines,
    /// Declared parameters.
    Params,
    /// Decisions plus one: each `if`, loop, extra `match` arm, `catch` arm,
    /// `&&`, `||`, `??` and `?.` (McCabe 1976).
    Cyclomatic,
    /// How hard the code is to follow: each branch or loop costs 1 plus its
    /// nesting depth (SonarSource 2017). Flatten nesting to lower it.
    Cognitive,
    /// The most branches and loops open at once.
    Nesting,
    /// Distinct paths through the function, which is how many cases a test
    /// suite needs to cover them all (Nejmeh 1988).
    Npath,
    /// Places the function can return: each `return` and `throw`, plus the
    /// end.
    Exits,
    /// Distinct functions in the project that call this one.
    FanIn,
    /// Distinct functions this one calls, stdlib included.
    FanOut,
}

impl Metric {
    fn name(self) -> String {
        self.to_possible_value()
            .map(|value| value.get_name().to_owned())
            .unwrap_or_default()
    }

    /// Whether this metric belongs to files rather than functions. `lines`
    /// is both; at file level it is the file's own count.
    fn file_only(self) -> bool {
        matches!(self, Self::Tests | Self::TestLines)
    }

    fn file_total(self) -> bool {
        matches!(self, Self::Lines | Self::Tests | Self::TestLines)
    }

    fn of_function(self, f: &FunctionMetrics) -> u64 {
        match self {
            Self::Lines => f.lines.into(),
            Self::Tests | Self::TestLines => 0,
            Self::Params => f.params.into(),
            Self::Cyclomatic => f.cyclomatic.into(),
            Self::Cognitive => f.cognitive.into(),
            Self::Nesting => f.nesting.into(),
            Self::Npath => f.npath,
            Self::Exits => f.exits.into(),
            Self::FanIn => f.fan_in.into(),
            Self::FanOut => f.fan_out.into(),
        }
    }

    fn of_file(self, f: &FileMetrics) -> u64 {
        match self {
            Self::Tests => f.tests.into(),
            Self::TestLines => f.test_lines.into(),
            _ => f.lines.into(),
        }
    }
}

const FUNCTION_DEFAULT: [Metric; 5] = [
    Metric::Cognitive,
    Metric::Nesting,
    Metric::Npath,
    Metric::Exits,
    Metric::Lines,
];

const FILE_DEFAULT: [Metric; 7] = [
    Metric::Cognitive,
    Metric::Nesting,
    Metric::Npath,
    Metric::Exits,
    Metric::Lines,
    Metric::Tests,
    Metric::TestLines,
];

impl MetricsArgs {
    pub(crate) fn run(&self) -> Result<crate::ExitCode> {
        let metrics = self.selected()?;
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
        let view = View::new(&session.db, &files, session.root(), &only);
        let out = if self.functions {
            view.functions(&metrics, self.all)
        } else if self.files {
            view.files(&metrics, self.all)
        } else {
            view.summary(&metrics)
        };
        print!("{out}");
        Ok(crate::ExitCode::Success)
    }

    fn selected(&self) -> Result<Vec<Metric>> {
        if self.metrics.is_empty() {
            return Ok(if self.functions {
                FUNCTION_DEFAULT.to_vec()
            } else {
                FILE_DEFAULT.to_vec()
            });
        }
        if self.functions
            && let Some(metric) = self.metrics.iter().find(|m| m.file_only())
        {
            bail!(
                "`{}` is measured per file; use it without --functions",
                metric.name()
            );
        }
        Ok(self.metrics.clone())
    }
}

/// The measured project, narrowed to the requested paths.
pub(crate) struct View<'a> {
    db: &'a ProjectDatabase,
    root: &'a Path,
    metrics: ProjectMetrics,
}

impl<'a> View<'a> {
    pub(crate) fn new(
        db: &'a ProjectDatabase,
        files: &[SourceFile],
        root: &'a Path,
        only: &[PathBuf],
    ) -> Self {
        let mut metrics = project_metrics(db, files);
        let keep =
            |file: SourceFile| only.is_empty() || only.iter().any(|p| file.path(db).starts_with(p));
        metrics.functions.retain(|f| keep(f.file));
        metrics.files.retain(|f| keep(f.file));
        Self { db, root, metrics }
    }

    fn path(&self, file: SourceFile) -> String {
        let path = file.path(self.db);
        path.strip_prefix(self.root)
            .unwrap_or(&path)
            .display()
            .to_string()
    }

    /// `path:line` of a function's first line.
    fn location(&self, f: &FunctionMetrics) -> String {
        let line = LineIndex::new(f.file.text(self.db)).line(f.span.start().into()) + 1;
        format!("{}:{line}", self.path(f.file))
    }

    pub(crate) fn summary(&self, metrics: &[Metric]) -> String {
        let functions = &self.metrics.functions;
        let mut out = format!(
            "{} functions in {} files\n",
            functions.len(),
            self.metrics.files.len()
        );

        let totals: Vec<Vec<String>> = metrics
            .iter()
            .filter(|m| m.file_total())
            .map(|&m| {
                let total: u64 = self.metrics.files.iter().map(|f| m.of_file(f)).sum();
                vec![m.name(), total.to_string()]
            })
            .collect();
        if !totals.is_empty() {
            out.push('\n');
            out.push_str(&table(&[String::new(), "total".into()], &totals, &[0]));
        }

        let spread: Vec<Vec<String>> = metrics
            .iter()
            .filter(|m| !m.file_total())
            .map(|&m| {
                // Reversed so a tie names the first function listed.
                let worst = functions
                    .iter()
                    .rev()
                    .max_by_key(|f| m.of_function(f))
                    .map(|f| format!("{} ({})", f.name, self.location(f)))
                    .unwrap_or_default();
                vec![
                    m.name(),
                    average(functions, m),
                    maximum(functions, m),
                    worst,
                ]
            })
            .collect();
        if !spread.is_empty() {
            out.push('\n');
            let header = ["", "avg", "max", "worst"].map(String::from);
            out.push_str(&table(&header, &spread, &[0, 3]));
        }

        out.push_str("\nMore detail: --files, or --functions [PATHS...]\n");
        out
    }

    pub(crate) fn files(&self, metrics: &[Metric], all: bool) -> String {
        let functions_in = |file: SourceFile| -> Vec<&FunctionMetrics> {
            self.metrics
                .functions
                .iter()
                .filter(|f| f.file == file)
                .collect()
        };
        let mut rows: Vec<(&FileMetrics, Vec<&FunctionMetrics>)> = self
            .metrics
            .files
            .iter()
            .map(|file| (file, functions_in(file.file)))
            .collect();
        let first = metrics[0];
        let key = |(file, functions): &(&FileMetrics, Vec<&FunctionMetrics>)| {
            if first.file_total() {
                first.of_file(file)
            } else {
                functions
                    .iter()
                    .map(|f| first.of_function(f))
                    .max()
                    .unwrap_or(0)
            }
        };
        rows.sort_by(|a, b| {
            key(b)
                .cmp(&key(a))
                .then_with(|| self.path(a.0.file).cmp(&self.path(b.0.file)))
        });

        let mut header = vec!["file".to_owned(), "functions".to_owned()];
        header.extend(metrics.iter().map(|m| {
            if m.file_total() {
                m.name()
            } else {
                format!("{} avg/max", m.name())
            }
        }));
        let shown = if all { rows.len() } else { rows.len().min(TOP) };
        let cells: Vec<Vec<String>> = rows[..shown]
            .iter()
            .map(|(file, functions)| {
                let mut row = vec![self.path(file.file), functions.len().to_string()];
                row.extend(metrics.iter().map(|&m| {
                    if m.file_total() {
                        m.of_file(file).to_string()
                    } else {
                        format!("{} / {}", average(functions, m), maximum(functions, m))
                    }
                }));
                row
            })
            .collect();

        let mut out = table(&header, &cells, &[0]);
        out.push_str(&footer(shown, rows.len(), "files"));
        out.push_str("More detail: --functions <PATHS...>\n");
        out
    }

    pub(crate) fn functions(&self, metrics: &[Metric], all: bool) -> String {
        let mut rows: Vec<&FunctionMetrics> = self.metrics.functions.iter().collect();
        let first = metrics[0];
        rows.sort_by(|a, b| {
            first
                .of_function(b)
                .cmp(&first.of_function(a))
                .then_with(|| self.location(a).cmp(&self.location(b)))
        });

        let mut header = vec!["location".to_owned(), "function".to_owned()];
        header.extend(metrics.iter().map(|m| m.name()));
        let shown = if all { rows.len() } else { rows.len().min(TOP) };
        let cells: Vec<Vec<String>> = rows[..shown]
            .iter()
            .map(|f| {
                let mut row = vec![self.location(f), f.name.to_string()];
                row.extend(metrics.iter().map(|m| m.of_function(f).to_string()));
                row
            })
            .collect();

        let mut out = table(&header, &cells, &[0, 1]);
        out.push_str(&footer(shown, rows.len(), "functions"));
        out
    }
}

fn average<F: std::borrow::Borrow<FunctionMetrics>>(functions: &[F], metric: Metric) -> String {
    if functions.is_empty() {
        return "-".to_owned();
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "an average shown to one decimal place"
    )]
    let total = functions
        .iter()
        .map(|f| metric.of_function(f.borrow()) as f64)
        .sum::<f64>();
    #[allow(
        clippy::cast_precision_loss,
        reason = "an average shown to one decimal place"
    )]
    let average = total / functions.len() as f64;
    // A decimal place only helps small numbers; `npath` averages run huge.
    if average < 100.0 {
        format!("{average:.1}")
    } else {
        format!("{average:.0}")
    }
}

fn maximum<F: std::borrow::Borrow<FunctionMetrics>>(functions: &[F], metric: Metric) -> String {
    functions
        .iter()
        .map(|f| metric.of_function(f.borrow()))
        .max()
        .map_or_else(|| "-".to_owned(), |max| max.to_string())
}

/// How many rows a list view cut, if any.
fn footer(shown: usize, total: usize, what: &str) -> String {
    if shown < total {
        format!("\n{shown} of {total} {what} shown; --all shows every row\n")
    } else {
        "\n".to_owned()
    }
}

/// Columns padded to their widest cell, two spaces apart: the `text` columns
/// left-aligned, the numeric rest right-aligned.
fn table(header: &[String], rows: &[Vec<String>], text: &[usize]) -> String {
    let mut widths: Vec<usize> = header.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let mut out = String::new();
    for row in std::iter::once(header).chain(rows.iter().map(Vec::as_slice)) {
        let cells: Vec<String> = row
            .iter()
            .zip(&widths)
            .enumerate()
            .map(|(i, (cell, width))| {
                if text.contains(&i) {
                    format!("{cell:<width$}")
                } else {
                    format!("{cell:>width$}")
                }
            })
            .collect();
        out.push_str(cells.join("  ").trim_end());
        out.push('\n');
    }
    out
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

    fn render(dir: &Path, only: &[&str], show: impl Fn(&View<'_>) -> String) -> String {
        let session = ProjectSession::open(Some(dir), CacheUse::Off).unwrap();
        let only: Vec<PathBuf> = only.iter().map(|p| session.root().join(p)).collect();
        let files = session.package.files(&session.db).clone();
        show(&View::new(&session.db, &files, session.root(), &only))
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

test \"pick\" {
    assert.equal(pick(2), \"many\");
}
";

    const CALLER: &str = "function twice() -> string {\n    pick(1) + pick(2)\n}\n";

    #[test]
    fn summary() {
        let dir = project(&[("main.baml", MAIN), ("caller.baml", CALLER)]);
        let out = render(dir.path(), &[], |view| view.summary(&FILE_DEFAULT));
        insta::assert_snapshot!(out, @"
        3 functions in 2 files

                    total
        lines          17
        tests           1
        test-lines      3

                   avg  max  worst
        cognitive  0.7    2  pick (baml_src/main.baml:1)
        nesting    0.3    1  pick (baml_src/main.baml:1)
        npath      1.3    2  pick (baml_src/main.baml:1)
        exits      1.0    1  twice (baml_src/caller.baml:1)

        More detail: --files, or --functions [PATHS...]
        ");
    }

    #[test]
    fn files_sort_by_first_metric() {
        let dir = project(&[("main.baml", MAIN), ("caller.baml", CALLER)]);
        let metrics = [Metric::Lines, Metric::Cognitive];
        let out = render(dir.path(), &[], |view| view.files(&metrics, false));
        insta::assert_snapshot!(out, @"
        file                  functions  lines  cognitive avg/max
        baml_src/main.baml            2     14            1.0 / 2
        baml_src/caller.baml          1      3            0.0 / 0

        More detail: --functions <PATHS...>
        ");
    }

    #[test]
    fn functions_keep_fan_in_from_filtered_files() {
        let dir = project(&[("main.baml", MAIN), ("caller.baml", CALLER)]);
        let metrics = [Metric::FanIn, Metric::FanOut, Metric::Lines];
        let out = render(dir.path(), &["baml_src/main.baml"], |view| {
            view.functions(&metrics, false)
        });
        // `twice` is filtered out but still counts as a caller of `pick`.
        insta::assert_snapshot!(out, @"
        location               function      fan-in  fan-out  lines
        baml_src/main.baml:1   pick               2        0      7
        baml_src/main.baml:12  Counter.bump       0        2      4
        ");
    }
}
