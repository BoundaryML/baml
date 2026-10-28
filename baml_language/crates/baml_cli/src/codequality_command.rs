//! `baml codequality`: code complexity metrics, broad to narrow. The default
//! view summarizes the project; `--files` gives one row per file and
//! `--functions` one row per function, worst first.

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};
use baml_db::{ProjectDatabase, SourceFile, line_index::LineIndex};
use baml_ide::{
    DefinitionKind,
    metrics::{FileMetrics, FunctionMetrics, ProjectMetrics, project_metrics},
};
use clap::{Args, ValueEnum};
use console::{Alignment, Style};
use serde::Serialize;

use crate::{
    paint::Painter,
    project_session::{CacheUse, ProjectSession},
    reporter::{Reporter, accent_style, secondary_style},
};

/// Rows a list view shows without `--all`.
const TOP: usize = 10;

/// Print code quality metrics.
#[derive(Args, Debug)]
#[command(after_long_help = "\
Examples:
  Summarize the project:
    baml codequality

  The files with the most complex functions:
    baml codequality --files

  Every function under one directory, by nesting depth:
    baml codequality --functions --all --metrics nesting,cognitive baml_src/agents

  Every metric as JSON, for other tools:
    baml codequality --output-format json")]
pub(crate) struct CodeQualityArgs {
    /// Only report functions and files under these paths. Calls from the rest
    /// of the project still count toward fan-in.
    pub(crate) paths: Vec<PathBuf>,

    /// One row per file.
    #[arg(long, conflicts_with = "functions", help_heading = "Output options")]
    pub(crate) files: bool,

    /// One row per function.
    #[arg(long, help_heading = "Output options")]
    pub(crate) functions: bool,

    /// Metrics to show, comma-separated. Rows sort worst first by the first
    /// one. Defaults to `cognitive,cyclomatic,nesting,npath,exits,lines`, plus
    /// `tests,test-lines` outside `--functions`.
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "LIST",
        help_heading = "Output options"
    )]
    pub(crate) metrics: Vec<Metric>,

    /// Show every row instead of the worst 10.
    #[arg(long, help_heading = "Output options")]
    pub(crate) all: bool,

    #[arg(
        long = "output-format",
        value_enum,
        value_name = "FORMAT",
        default_value_t = OutputFormat::Table,
        help = "Format the report; json holds every function and file with every metric, so it takes no other output option [default: table] [possible values: table, json]",
        hide_default_value = true,
        hide_possible_values = true,
        help_heading = "Output options"
    )]
    pub(crate) output_format: OutputFormat,

    /// Set from the global `--project`.
    #[arg(skip)]
    pub(crate) from: Option<PathBuf>,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    /// Tables for reading.
    Table,
    /// One JSON document with every function and file.
    Json,
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

    /// The name as a heading reads it: `test lines`, not `test-lines`.
    fn label(self) -> String {
        self.name().replace('-', " ")
    }

    /// Whether summing it over functions means anything. A sum of nesting
    /// depths or of path counts does not.
    fn additive(self) -> bool {
        !matches!(self, Self::Nesting | Self::Npath)
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

const FUNCTION_DEFAULT: [Metric; 6] = [
    Metric::Cognitive,
    Metric::Cyclomatic,
    Metric::Nesting,
    Metric::Npath,
    Metric::Exits,
    Metric::Lines,
];

const FILE_DEFAULT: [Metric; 8] = [
    Metric::Cognitive,
    Metric::Cyclomatic,
    Metric::Nesting,
    Metric::Npath,
    Metric::Exits,
    Metric::Lines,
    Metric::Tests,
    Metric::TestLines,
];

impl CodeQualityArgs {
    pub(crate) fn run(&self) -> Result<crate::ExitCode> {
        let metrics = self.selected()?;
        let reporter = Reporter::new();
        let session = ProjectSession::open(self.from.as_deref(), CacheUse::Off)?;
        if session.is_empty() {
            reporter.abandon();
            crate::reporter::print_error(format_args!(
                "no .baml files found in {}",
                session.root().display()
            ));
            return Ok(crate::ExitCode::Other);
        }
        let only = self
            .paths
            .iter()
            // Canonical, as the project loader stores source paths, so a
            // symlink or `..` in the argument still matches.
            .map(|path| {
                std::fs::canonicalize(path)
                    .with_context(|| format!("path not found: {}", path.display()))
            })
            .collect::<Result<Vec<_>>>()?;
        reporter.spin("Measuring", format!("{} file(s)", session.file_count()));
        let files = session.package.files(&session.db).clone();
        let view = View::new(&session.db, &files, session.root(), &only);
        reporter.finish(
            "Finished",
            format!(
                "measured {} function(s) in {} file(s)",
                view.metrics.functions.len(),
                view.metrics.files.len()
            ),
        );

        let mut out = std::io::stdout().lock();
        if self.output_format == OutputFormat::Json {
            serde_json::to_writer_pretty(&mut out, &view.dump())?;
            writeln!(out)?;
            return Ok(crate::ExitCode::Success);
        }
        let painter = Painter::stdout();
        if self.functions {
            view.write_functions(&mut out, &painter, &metrics, self.all)?;
        } else if self.files {
            view.write_files(&mut out, &painter, &metrics, self.all)?;
        } else {
            view.write_summary(&mut out, &painter, &metrics)?;
        }
        Ok(crate::ExitCode::Success)
    }

    fn selected(&self) -> Result<Vec<Metric>> {
        if self.output_format == OutputFormat::Json
            && (self.files || self.functions || self.all || !self.metrics.is_empty())
        {
            bail!(
                "--output-format json always includes every function, file and metric; drop --files, --functions, --metrics and --all"
            );
        }
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

    fn line(&self, f: &FunctionMetrics) -> u32 {
        LineIndex::new(f.file.text(self.db)).line(f.span.start().into()) + 1
    }

    /// Every function and file with every metric, for `--output-format json`.
    pub(crate) fn dump(&self) -> Dump {
        Dump {
            functions: self
                .metrics
                .functions
                .iter()
                .map(|f| {
                    let lines = LineIndex::new(f.file.text(self.db));
                    FunctionRow {
                        name: f.name.to_string(),
                        file: self.path(f.file),
                        start_line: lines.line(f.span.start().into()) + 1,
                        // The span's end is exclusive.
                        end_line: lines.line(u32::from(f.span.end()).saturating_sub(1)) + 1,
                        lines: f.lines,
                        params: f.params,
                        cyclomatic: f.cyclomatic,
                        cognitive: f.cognitive,
                        nesting: f.nesting,
                        npath: f.npath,
                        exits: f.exits,
                        fan_in: f.fan_in,
                        fan_out: f.fan_out,
                    }
                })
                .collect(),
            files: self
                .metrics
                .files
                .iter()
                .map(|f| FileRow {
                    file: self.path(f.file),
                    lines: f.lines,
                    tests: f.tests,
                    test_lines: f.test_lines,
                })
                .collect(),
        }
    }

    /// `path:line` of a function's first line, a hyperlink where the
    /// terminal supports one.
    fn location(&self, painter: &Painter, f: &FunctionMetrics) -> String {
        painter.location(
            &f.file.path(self.db),
            &self.path(f.file),
            &self.line(f).to_string(),
        )
    }

    fn name(painter: &Painter, f: &FunctionMetrics) -> String {
        let kind = if f.name.contains('.') {
            DefinitionKind::Method
        } else {
            DefinitionKind::Function
        };
        painter.fqn(&f.name, kind)
    }

    pub(crate) fn write_summary(
        &self,
        w: &mut impl Write,
        painter: &Painter,
        metrics: &[Metric],
    ) -> std::io::Result<()> {
        let styles = Styles::new(painter);
        let functions = &self.metrics.functions;

        let mut overview = vec![
            vec!["functions".to_owned(), count(functions.len() as u64)],
            vec!["files".to_owned(), count(self.metrics.files.len() as u64)],
        ];
        overview.extend(metrics.iter().filter(|m| m.file_total()).map(|&m| {
            let total: u64 = self.metrics.files.iter().map(|f| m.of_file(f)).sum();
            vec![m.label(), count(total)]
        }));
        writeln!(w, "{}", styles.header.apply_to("Overview"))?;
        styles.table(w, None, &overview, &[0])?;
        writeln!(w)?;

        let spread: Vec<Vec<String>> = metrics
            .iter()
            .filter(|m| !m.file_total())
            .map(|&m| {
                // Reversed so a tie names the first function listed.
                let worst = functions.iter().rev().max_by_key(|f| m.of_function(f));
                let (name, location) = worst
                    .map(|f| (Self::name(painter, f), self.location(painter, f)))
                    .unwrap_or_default();
                vec![
                    m.label(),
                    total(functions, m),
                    average(functions, m),
                    maximum(functions, m),
                    name,
                    styles.dim.apply_to(location).to_string(),
                ]
            })
            .collect();
        if !spread.is_empty() {
            writeln!(w, "{}", styles.header.apply_to("Complexity"))?;
            let header = ["", "total", "avg", "max", "worst", ""].map(String::from);
            styles.table(w, Some(&header[..]), &spread, &[0, 4, 5])?;
            writeln!(w)?;
        }

        styles.hint(w, "More detail: --files, or --functions [PATHS...]")
    }

    pub(crate) fn write_files(
        &self,
        w: &mut impl Write,
        painter: &Painter,
        metrics: &[Metric],
        all: bool,
    ) -> std::io::Result<()> {
        let styles = Styles::new(painter);
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
                m.label()
            } else {
                format!("{} avg/max", m.label())
            }
        }));
        let shown = if all { rows.len() } else { rows.len().min(TOP) };
        let cells: Vec<Vec<String>> = rows[..shown]
            .iter()
            .map(|(file, functions)| {
                let mut row = vec![self.path(file.file), count(functions.len() as u64)];
                row.extend(metrics.iter().map(|&m| {
                    if m.file_total() {
                        count(m.of_file(file))
                    } else {
                        format!("{} / {}", average(functions, m), maximum(functions, m))
                    }
                }));
                row
            })
            .collect();

        writeln!(
            w,
            "{}",
            styles
                .header
                .apply_to(format!("Files, worst {} first", first.label()))
        )?;
        styles.table(w, Some(header.as_slice()), &cells, &[0])?;
        writeln!(w)?;
        styles.cut(w, shown, rows.len(), "files")?;
        styles.hint(w, "More detail: --functions <PATHS...>")
    }

    pub(crate) fn write_functions(
        &self,
        w: &mut impl Write,
        painter: &Painter,
        metrics: &[Metric],
        all: bool,
    ) -> std::io::Result<()> {
        let styles = Styles::new(painter);
        let mut rows: Vec<&FunctionMetrics> = self.metrics.functions.iter().collect();
        let first = metrics[0];
        rows.sort_by(|a, b| {
            first
                .of_function(b)
                .cmp(&first.of_function(a))
                .then_with(|| self.path(a.file).cmp(&self.path(b.file)))
                .then_with(|| self.line(a).cmp(&self.line(b)))
        });

        let mut header = vec!["function".to_owned(), "location".to_owned()];
        header.extend(metrics.iter().map(|m| m.label()));
        let shown = if all { rows.len() } else { rows.len().min(TOP) };
        let cells: Vec<Vec<String>> = rows[..shown]
            .iter()
            .map(|f| {
                let location = styles.dim.apply_to(self.location(painter, f)).to_string();
                let mut row = vec![Self::name(painter, f), location];
                row.extend(metrics.iter().map(|m| count(m.of_function(f))));
                row
            })
            .collect();

        writeln!(
            w,
            "{}",
            styles
                .header
                .apply_to(format!("Functions, worst {} first", first.label()))
        )?;
        styles.table(w, Some(header.as_slice()), &cells, &[0, 1])?;
        writeln!(w)?;
        styles.cut(w, shown, rows.len(), "functions")
    }
}

/// The `--output-format json` document.
#[derive(Debug, Serialize)]
pub(crate) struct Dump {
    functions: Vec<FunctionRow>,
    files: Vec<FileRow>,
}

#[derive(Debug, Serialize)]
struct FunctionRow {
    /// `Owner.method` for methods.
    name: String,
    file: String,
    /// One-based and inclusive.
    start_line: u32,
    end_line: u32,
    lines: u32,
    params: u32,
    cyclomatic: u32,
    cognitive: u32,
    nesting: u32,
    /// Saturates at `u64::MAX`.
    npath: u64,
    exits: u32,
    fan_in: u32,
    fan_out: u32,
}

#[derive(Debug, Serialize)]
struct FileRow {
    file: String,
    lines: u32,
    tests: u32,
    test_lines: u32,
}

/// The styles one render uses, colored only when its stream is.
struct Styles {
    header: Style,
    dim: Style,
}

impl Styles {
    fn new(painter: &Painter) -> Self {
        let color = painter.enabled();
        Self {
            header: accent_style().force_styling(color),
            dim: secondary_style().force_styling(color),
        }
    }

    /// An indented table: the dim header row if any, then each row, every
    /// column padded to its widest cell. The `text` columns are left-aligned
    /// and the numeric rest right-aligned. Cells may already be styled; widths
    /// are measured without their escape codes.
    fn table(
        &self,
        w: &mut impl Write,
        header: Option<&[String]>,
        rows: &[Vec<String>],
        text: &[usize],
    ) -> std::io::Result<()> {
        let mut widths: Vec<usize> = Vec::new();
        for row in header.into_iter().chain(rows.iter().map(Vec::as_slice)) {
            widths.resize(widths.len().max(row.len()), 0);
            for (width, cell) in widths.iter_mut().zip(row) {
                *width = (*width).max(console::measure_text_width(cell));
            }
        }
        let line = |row: &[String]| {
            let cells: Vec<String> = row
                .iter()
                .zip(&widths)
                .enumerate()
                .map(|(i, (cell, &width))| {
                    let align = if text.contains(&i) {
                        Alignment::Left
                    } else {
                        Alignment::Right
                    };
                    console::pad_str(cell, width, align, None).into_owned()
                })
                .collect();
            cells.join("  ").trim_end().to_owned()
        };
        if let Some(header) = header {
            writeln!(w, "  {}", self.dim.apply_to(line(header)))?;
        }
        for row in rows {
            writeln!(w, "  {}", line(row))?;
        }
        Ok(())
    }

    /// How many rows a list view cut, if any.
    fn cut(
        &self,
        w: &mut impl Write,
        shown: usize,
        total: usize,
        what: &str,
    ) -> std::io::Result<()> {
        if shown < total {
            self.hint(
                w,
                &format!("{shown} of {total} {what} shown. Pass --all for every row."),
            )?;
        }
        Ok(())
    }

    fn hint(&self, w: &mut impl Write, text: &str) -> std::io::Result<()> {
        writeln!(w, "{}", self.dim.apply_to(text))
    }
}

fn total<F: std::borrow::Borrow<FunctionMetrics>>(functions: &[F], metric: Metric) -> String {
    if !metric.additive() {
        return "-".to_owned();
    }
    let sum = functions
        .iter()
        .map(|f| metric.of_function(f.borrow()))
        .fold(0u64, u64::saturating_add);
    count(sum)
}

fn average<F: std::borrow::Borrow<FunctionMetrics>>(functions: &[F], metric: Metric) -> String {
    if functions.is_empty() {
        return "-".to_owned();
    }
    #[allow(clippy::cast_precision_loss, reason = "an average, shown rounded")]
    let average = functions
        .iter()
        .map(|f| metric.of_function(f.borrow()) as f64)
        .sum::<f64>()
        / functions.len() as f64;
    if average < 100.0 {
        format!("{average:.1}")
    } else {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a positive average, shown rounded"
        )]
        let rounded = average.round() as u64;
        count(rounded)
    }
}

fn maximum<F: std::borrow::Borrow<FunctionMetrics>>(functions: &[F], metric: Metric) -> String {
    functions
        .iter()
        .map(|f| metric.of_function(f.borrow()))
        .max()
        .map_or_else(|| "-".to_owned(), count)
}

/// `n` for a reader: digits grouped by thousands up to a million, then one
/// decimal and a suffix (`2.7M`, `273.0B`), then scientific notation past a
/// thousand trillion, where `npath` ends up.
fn count(n: u64) -> String {
    #[allow(clippy::cast_precision_loss, reason = "shown to one decimal place")]
    let x = n as f64;
    match n {
        0..1_000_000 => {
            let digits = n.to_string();
            let mut out = String::new();
            for (i, digit) in digits.chars().enumerate() {
                if i > 0 && (digits.len() - i).is_multiple_of(3) {
                    out.push(',');
                }
                out.push(digit);
            }
            out
        }
        1_000_000..1_000_000_000 => format!("{:.1}M", x / 1e6),
        1_000_000_000..1_000_000_000_000 => format!("{:.1}B", x / 1e9),
        1_000_000_000_000..1_000_000_000_000_000 => format!("{:.1}T", x / 1e12),
        _ => format!("{x:.1e}"),
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

    fn render(
        dir: &Path,
        only: &[&str],
        show: impl Fn(&View<'_>, &mut Vec<u8>, &Painter) -> std::io::Result<()>,
    ) -> String {
        let session = ProjectSession::open(Some(dir), CacheUse::Off).unwrap();
        let only: Vec<PathBuf> = only.iter().map(|p| session.root().join(p)).collect();
        let files = session.package.files(&session.db).clone();
        let view = View::new(&session.db, &files, session.root(), &only);
        let mut out = Vec::new();
        show(&view, &mut out, &Painter::stdout()).unwrap();
        // Paths print with the native separator; the snapshots use `/`.
        String::from_utf8(out)
            .unwrap()
            .replace(std::path::MAIN_SEPARATOR, "/")
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
    fn counts_read_as_people_write_them() {
        let shown: Vec<String> = [0, 999, 3_170, 999_999, 2_700_000, 273_049_159_680, u64::MAX]
            .into_iter()
            .map(count)
            .collect();
        assert_eq!(
            shown,
            ["0", "999", "3,170", "999,999", "2.7M", "273.0B", "1.8e19"]
        );
    }

    #[test]
    fn summary() {
        let dir = project(&[("main.baml", MAIN), ("caller.baml", CALLER)]);
        let out = render(dir.path(), &[], |view, w, painter| {
            view.write_summary(w, painter, &FILE_DEFAULT)
        });
        insta::assert_snapshot!(out, @"
        Overview
          functions    3
          files        2
          lines       17
          tests        1
          test lines   3

        Complexity
                      total  avg  max  worst
          cognitive       2  0.7    2  pick   baml_src/main.baml:1
          cyclomatic      4  1.3    2  pick   baml_src/main.baml:1
          nesting         -  0.3    1  pick   baml_src/main.baml:1
          npath           -  1.3    2  pick   baml_src/main.baml:1
          exits           3  1.0    1  twice  baml_src/caller.baml:1

        More detail: --files, or --functions [PATHS...]
        ");
    }

    #[test]
    fn json_dumps_every_function_and_file() {
        let dir = project(&[("main.baml", MAIN), ("caller.baml", CALLER)]);
        // Normalized before serializing: JSON escapes a Windows `\` to `\\`,
        // which the replace in `render` would turn into `//`.
        let out = render(dir.path(), &[], |view, w, _| {
            let mut dump = view.dump();
            for f in &mut dump.functions {
                f.file = f.file.replace(std::path::MAIN_SEPARATOR, "/");
            }
            for f in &mut dump.files {
                f.file = f.file.replace(std::path::MAIN_SEPARATOR, "/");
            }
            serde_json::to_writer_pretty(&mut *w, &dump)?;
            Ok(())
        });
        insta::assert_snapshot!(out, @r#"
        {
          "functions": [
            {
              "name": "twice",
              "file": "baml_src/caller.baml",
              "start_line": 1,
              "end_line": 3,
              "lines": 3,
              "params": 0,
              "cyclomatic": 1,
              "cognitive": 0,
              "nesting": 0,
              "npath": 1,
              "exits": 1,
              "fan_in": 0,
              "fan_out": 1
            },
            {
              "name": "pick",
              "file": "baml_src/main.baml",
              "start_line": 1,
              "end_line": 7,
              "lines": 7,
              "params": 1,
              "cyclomatic": 2,
              "cognitive": 2,
              "nesting": 1,
              "npath": 2,
              "exits": 1,
              "fan_in": 2,
              "fan_out": 0
            },
            {
              "name": "Counter.bump",
              "file": "baml_src/main.baml",
              "start_line": 12,
              "end_line": 15,
              "lines": 4,
              "params": 1,
              "cyclomatic": 1,
              "cognitive": 0,
              "nesting": 0,
              "npath": 1,
              "exits": 1,
              "fan_in": 0,
              "fan_out": 2
            }
          ],
          "files": [
            {
              "file": "baml_src/caller.baml",
              "lines": 3,
              "tests": 0,
              "test_lines": 0
            },
            {
              "file": "baml_src/main.baml",
              "lines": 14,
              "tests": 1,
              "test_lines": 3
            }
          ]
        }
        "#);
    }

    #[test]
    fn files_sort_by_first_metric() {
        let dir = project(&[("main.baml", MAIN), ("caller.baml", CALLER)]);
        let metrics = [Metric::Lines, Metric::Cognitive];
        let out = render(dir.path(), &[], |view, w, painter| {
            view.write_files(w, painter, &metrics, false)
        });
        insta::assert_snapshot!(out, @"
        Files, worst lines first
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
        let out = render(dir.path(), &["baml_src/main.baml"], |view, w, painter| {
            view.write_functions(w, painter, &metrics, false)
        });
        // `twice` is filtered out but still counts as a caller of `pick`.
        insta::assert_snapshot!(out, @"
        Functions, worst fan in first
          function      location               fan in  fan out  lines
          pick          baml_src/main.baml:1        2        0      7
          Counter.bump  baml_src/main.baml:12       0        2      4
        ");
    }
}
