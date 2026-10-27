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

  Every function in one file, by nesting depth:
    baml codequality --functions --all --metrics nesting,cognitive baml_src/router.baml")]
pub(crate) struct CodeQualityArgs {
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
            .map(|path| {
                std::path::absolute(path)
                    .with_context(|| format!("could not resolve path: {}", path.display()))
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

        let painter = Painter::stdout();
        let mut out = std::io::stdout().lock();
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

        let totals: Vec<Vec<String>> = metrics
            .iter()
            .filter(|m| m.file_total())
            .map(|&m| {
                let total: u64 = self.metrics.files.iter().map(|f| m.of_file(f)).sum();
                vec![m.name(), total.to_string()]
            })
            .collect();
        if !totals.is_empty() {
            writeln!(w, "{}", styles.header.apply_to("Size"))?;
            let header = ["", "total"].map(String::from);
            styles.table(w, &header, &totals, &[0])?;
            writeln!(w)?;
        }

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
                    m.name(),
                    average(functions, m),
                    maximum(functions, m),
                    name,
                    styles.dim.apply_to(location).to_string(),
                ]
            })
            .collect();
        if !spread.is_empty() {
            writeln!(w, "{}", styles.header.apply_to("Complexity"))?;
            let header = ["", "avg", "max", "worst", ""].map(String::from);
            styles.table(w, &header, &spread, &[0, 3, 4])?;
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

        writeln!(
            w,
            "{}",
            styles
                .header
                .apply_to(format!("Files, worst {} first", first.name()))
        )?;
        styles.table(w, &header, &cells, &[0])?;
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
        header.extend(metrics.iter().map(|m| m.name()));
        let shown = if all { rows.len() } else { rows.len().min(TOP) };
        let cells: Vec<Vec<String>> = rows[..shown]
            .iter()
            .map(|f| {
                let location = styles.dim.apply_to(self.location(painter, f)).to_string();
                let mut row = vec![Self::name(painter, f), location];
                row.extend(metrics.iter().map(|m| m.of_function(f).to_string()));
                row
            })
            .collect();

        writeln!(
            w,
            "{}",
            styles
                .header
                .apply_to(format!("Functions, worst {} first", first.name()))
        )?;
        styles.table(w, &header, &cells, &[0, 1])?;
        writeln!(w)?;
        styles.cut(w, shown, rows.len(), "functions")
    }
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

    /// An indented table: the dim header row, then each row, every column
    /// padded to its widest cell. The `text` columns are left-aligned and the
    /// numeric rest right-aligned. Cells may already be styled; widths are
    /// measured without their escape codes.
    fn table(
        &self,
        w: &mut impl Write,
        header: &[String],
        rows: &[Vec<String>],
        text: &[usize],
    ) -> std::io::Result<()> {
        let mut widths: Vec<usize> = header
            .iter()
            .map(|h| console::measure_text_width(h))
            .collect();
        for row in rows {
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
        writeln!(w, "  {}", self.dim.apply_to(line(header)))?;
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
        String::from_utf8(out).unwrap()
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
        let out = render(dir.path(), &[], |view, w, painter| {
            view.write_summary(w, painter, &FILE_DEFAULT)
        });
        insta::assert_snapshot!(out, @"
        Size
                      total
          lines          17
          tests           1
          test-lines      3

        Complexity
                     avg  max  worst
          cognitive  0.7    2  pick   baml_src/main.baml:1
          nesting    0.3    1  pick   baml_src/main.baml:1
          npath      1.3    2  pick   baml_src/main.baml:1
          exits      1.0    1  twice  baml_src/caller.baml:1

        More detail: --files, or --functions [PATHS...]
        ");
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
        Functions, worst fan-in first
          function      location               fan-in  fan-out  lines
          pick          baml_src/main.baml:1        2        0      7
          Counter.bump  baml_src/main.baml:12       0        2      4
        ");
    }
}
