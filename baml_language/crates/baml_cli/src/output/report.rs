//! The command contract: commands return values, the dispatcher owns stdout.
//!
//! A command never writes stdout itself. It returns a [`Report`], and the
//! dispatcher renders it with [`Report::text`] or serde, depending on the
//! global `--output-format`. Status, progress and diagnostics go to stderr
//! through the reporter. [`RawCommand`] is the only exemption: `lsp` speaks a
//! protocol on stdio and `run` passes the program's own stdout through.

use std::{
    io::{self, Write},
    path::PathBuf,
};

use clap::ValueEnum;
use serde::Serialize;

use crate::ExitCode;

/// What goes to stdout. Nothing switches it implicitly: not the preset, not
/// agent detection, not a tty check.
#[derive(ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    /// Human rendering; may change between releases.
    #[default]
    Text,
    /// One JSON document; its schema only grows.
    Json,
    /// One JSON item per line; only for commands whose result is a sequence.
    Jsonl,
}

impl From<baml_exec::OutputFormat> for OutputFormat {
    fn from(format: baml_exec::OutputFormat) -> Self {
        match format {
            baml_exec::OutputFormat::Text => Self::Text,
            baml_exec::OutputFormat::Json => Self::Json,
        }
    }
}

/// How text output is styled. Only color and hyperlinks, so stdout reads the
/// same for humans and agents.
pub(crate) type Style = crate::paint::Painter;

/// What a command gets from the dispatcher. Deliberately not the terminal.
pub(crate) struct CommandContext {
    /// The global `--project-dir`.
    pub project_dir: Option<PathBuf>,
    pub format: OutputFormat,
}

#[cfg(test)]
impl CommandContext {
    pub(crate) fn for_test(project_dir: Option<&std::path::Path>) -> Self {
        Self {
            project_dir: project_dir.map(std::path::Path::to_path_buf),
            format: OutputFormat::Text,
        }
    }
}

/// A report's plain text rendering.
#[cfg(test)]
pub(crate) fn text_of(report: &impl Report) -> String {
    let mut out = Vec::new();
    report
        .text(&mut out, &Style::new(false, false))
        .expect("writing to a buffer cannot fail");
    String::from_utf8(out).expect("reports render UTF-8")
}

/// A command's result. JSON comes from `Serialize`, never by hand; fields that
/// only shape the text view are `#[serde(skip)]`.
pub(crate) trait Report: Serialize {
    /// Whether [`Report::jsonl`] is implemented.
    const JSONL: bool = false;

    fn text(&self, out: &mut dyn Write, style: &Style) -> io::Result<()>;

    /// The report as a sequence of JSON lines. Only called when
    /// [`Report::JSONL`] is set.
    fn jsonl(&self) -> Vec<serde_json::Value> {
        unreachable!("jsonl requested from a report without JSONL")
    }
}

/// Nothing to report: no stdout in text mode, `null` in JSON.
impl Report for () {
    fn text(&self, _out: &mut dyn Write, _style: &Style) -> io::Result<()> {
        Ok(())
    }
}

/// A value plus the exit status it should produce. Failing commands still
/// return a value, so `check --output-format json` reports what failed.
#[derive(Debug)]
pub(crate) struct Outcome<T> {
    pub value: T,
    pub status: ExitCode,
}

impl<T> Outcome<T> {
    pub(crate) fn ok(value: T) -> Self {
        Self {
            value,
            status: ExitCode::Success,
        }
    }

    pub(crate) fn new(value: T, status: ExitCode) -> Self {
        Self { value, status }
    }
}

/// A command that produces one result.
pub(crate) trait Command {
    type Output: Report;
    fn run(&self, cx: &CommandContext) -> anyhow::Result<Outcome<Self::Output>>;
}

/// A command that owns stdout. Only `lsp` and `run`.
pub(crate) trait RawCommand {
    fn run(&self, cx: &CommandContext) -> anyhow::Result<ExitCode>;
}

pub(crate) fn run_command<C: Command>(
    command: &C,
    cx: &CommandContext,
) -> anyhow::Result<ExitCode> {
    if cx.format == OutputFormat::Jsonl && !C::Output::JSONL {
        return Ok(jsonl_unsupported());
    }
    let outcome = command.run(cx)?;
    write(cx.format, &outcome.value)?;
    Ok(outcome.status)
}

pub(crate) fn run_raw<C: RawCommand>(command: &C, cx: &CommandContext) -> anyhow::Result<ExitCode> {
    if cx.format == OutputFormat::Jsonl {
        return Ok(jsonl_unsupported());
    }
    command.run(cx)
}

/// Render one value to stdout in the selected format.
pub(crate) fn write<T: Report>(format: OutputFormat, value: &T) -> anyhow::Result<()> {
    let mut out = io::stdout().lock();
    match format {
        OutputFormat::Text => value.text(&mut out, &super::stdout_style())?,
        OutputFormat::Json => {
            serde_json::to_writer_pretty(&mut out, value)?;
            writeln!(out)?;
        }
        OutputFormat::Jsonl if T::JSONL => {
            for line in value.jsonl() {
                serde_json::to_writer(&mut out, &line)?;
                writeln!(out)?;
            }
        }
        OutputFormat::Jsonl => {
            serde_json::to_writer(&mut out, value)?;
            writeln!(out)?;
        }
    }
    out.flush()?;
    Ok(())
}

fn jsonl_unsupported() -> ExitCode {
    crate::reporter::print_error(
        "`--output-format jsonl` is only supported by `test` and `recordings query`; use `json`",
    );
    ExitCode::Usage
}

#[cfg(test)]
mod tests {
    /// Only the dispatcher writes stdout, and text is styled only through
    /// the [`super::Style`] it hands to `Report::text` (or a stderr painter).
    /// `log_output` is the one other stdout writer: it passes a running
    /// program's `log.*` events through for `run`.
    #[test]
    fn only_the_output_module_writes_or_styles_stdout() {
        fn visit(dir: &std::path::Path, offenders: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    if path.file_name().is_some_and(|name| name != "output") {
                        visit(&path, offenders);
                    }
                    continue;
                }
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("");
                // `paint` and `reporter` are where the painters are defined.
                if matches!(name, "main.rs" | "paint.rs" | "reporter.rs")
                    || path.extension().is_none_or(|ext| ext != "rs")
                {
                    continue;
                }
                let may_write = name == "log_output.rs";
                let source = std::fs::read_to_string(&path).unwrap();
                // Unit tests may print; the command code above them may not.
                let code = source.split("#[cfg(test)]").next().unwrap_or(&source);
                for (number, line) in code.lines().enumerate() {
                    let line = line.trim_start();
                    let writes_stdout = !line.starts_with("//")
                        && (line.contains("println!(") && !line.contains("eprintln!(")
                            || line.contains("print!(")
                                && !line.contains("eprint!(")
                                && !line.contains("println!(")
                            || line.contains("io::stdout()"));
                    // `console`'s free styling reads a process-wide color
                    // switch instead of the stream's resolved policy.
                    let styles_globally = !line.starts_with("//")
                        && (line.contains("console::style")
                            || line.contains("console::Style")
                            || line.contains("accent_style()")
                            || line.contains("secondary_style()"));
                    if writes_stdout && !may_write || styles_globally {
                        offenders.push(format!("{}:{}", path.display(), number + 1));
                    }
                }
            }
        }
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        visit(&src, &mut offenders);
        assert!(
            offenders.is_empty(),
            "return a `Report` and style it with the `Style` passed to `text`: {offenders:#?}"
        );
    }
}
