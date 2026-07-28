//! Terminal output and root-help protocol shared by BAML command-line surfaces.

use std::{
    fmt,
    io::{self, Write},
    sync::atomic::{AtomicU8, Ordering},
};

use anstream::AutoStream;
use anstyle::{AnsiColor, Color, Effects, RgbColor, Style};
use serde::{Deserialize, Serialize};

pub const ROOT_HELP_COMMAND_V1: &str = "__baml-root-help-v1";
pub const ROOT_HELP_SCHEMA_V1: &str = "baml.root-help.v1";

const AUTO: u8 = 0;
const ALWAYS: u8 = 1;
const NEVER: u8 = 2;
const STATUS: Style = Style::new()
    .fg_color(Some(Color::Rgb(RgbColor(0xA8, 0x55, 0xF7))))
    .effects(Effects::BOLD);
const ERROR: Style = AnsiColor::Red.on_default().effects(Effects::BOLD);
const WARNING: Style = AnsiColor::Yellow.on_default().effects(Effects::BOLD);
const NOTE: Style = AnsiColor::Cyan.on_default().effects(Effects::BOLD);
const GOOD: Style = AnsiColor::Green.on_default().effects(Effects::BOLD);
const DIM: Style = Style::new().effects(Effects::DIMMED);
const HEADING: Style = STATUS;
const LITERAL: Style = Style::new().effects(Effects::BOLD);
static STDOUT_COLOR: AtomicU8 = AtomicU8::new(AUTO);
static STDERR_COLOR: AtomicU8 = AtomicU8::new(AUTO);

#[cfg(feature = "clap")]
pub const CLAP_STYLING: clap::builder::styling::Styles = {
    use clap::builder::styling::{AnsiColor, Color, Effects, RgbColor, Style, Styles};
    const PURPLE: Color = Color::Rgb(RgbColor(0xA8, 0x55, 0xF7));
    const PURPLE_LIGHT: Color = Color::Rgb(RgbColor(0xE9, 0xD5, 0xFF));
    Styles::styled()
        .header(Style::new().fg_color(Some(PURPLE)).effects(Effects::BOLD))
        .usage(Style::new().fg_color(Some(PURPLE)).effects(Effects::BOLD))
        .literal(Style::new().effects(Effects::BOLD))
        .placeholder(Style::new().fg_color(Some(PURPLE_LIGHT)))
        .error(
            Style::new()
                .fg_color(Some(Color::Ansi(AnsiColor::Red)))
                .effects(Effects::BOLD),
        )
        .valid(
            Style::new()
                .fg_color(Some(Color::Ansi(AnsiColor::Green)))
                .effects(Effects::BOLD),
        )
        .invalid(
            Style::new()
                .fg_color(Some(Color::Ansi(AnsiColor::Yellow)))
                .effects(Effects::BOLD),
        )
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    fn encode(self) -> u8 {
        match self {
            Self::Auto => AUTO,
            Self::Always => ALWAYS,
            Self::Never => NEVER,
        }
    }

    fn decode(value: u8) -> Self {
        match value {
            ALWAYS => Self::Always,
            NEVER => Self::Never,
            _ => Self::Auto,
        }
    }

    fn anstream(self) -> anstream::ColorChoice {
        match self {
            Self::Auto => anstream::ColorChoice::Auto,
            Self::Always => anstream::ColorChoice::Always,
            Self::Never => anstream::ColorChoice::Never,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeStyle {
    Heading,
    Good,
    Bad,
    Warning,
    Note,
    Dim,
}

impl ThemeStyle {
    fn style(self) -> &'static Style {
        match self {
            Self::Heading => &HEADING,
            Self::Good => &GOOD,
            Self::Bad => &ERROR,
            Self::Warning => &WARNING,
            Self::Note => &NOTE,
            Self::Dim => &DIM,
        }
    }
}

pub fn set_default_color_choices(stdout: ColorChoice, stderr: ColorChoice) {
    STDOUT_COLOR.store(stdout.encode(), Ordering::Relaxed);
    STDERR_COLOR.store(stderr.encode(), Ordering::Relaxed);
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct RootHelpV1 {
    pub schema_version: String,
    pub name: String,
    pub version: String,
    pub about: String,
    pub usage: String,
    pub commands: Vec<HelpRow>,
    pub options: Vec<HelpRow>,
}

impl RootHelpV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != ROOT_HELP_SCHEMA_V1 {
            return Err("unsupported root-help schema");
        }
        if self.name.is_empty() || self.usage.is_empty() {
            return Err("incomplete root-help metadata");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct HelpRow {
    pub syntax: String,
    pub summary: String,
}

pub struct Shell {
    output: ShellOut,
}

enum ShellOut {
    Streams {
        stdout: AutoStream<io::Stdout>,
        stderr: AutoStream<io::Stderr>,
    },
    Write(AutoStream<Box<dyn Write + Send + Sync>>),
}

impl Shell {
    pub fn new() -> Self {
        Self::with_color_choices(
            ColorChoice::decode(STDOUT_COLOR.load(Ordering::Relaxed)),
            ColorChoice::decode(STDERR_COLOR.load(Ordering::Relaxed)),
        )
    }

    pub fn with_color_choices(stdout: ColorChoice, stderr: ColorChoice) -> Self {
        Self {
            output: ShellOut::Streams {
                stdout: AutoStream::new(io::stdout(), stdout.anstream()),
                stderr: AutoStream::new(io::stderr(), stderr.anstream()),
            },
        }
    }

    pub fn from_write(output: Box<dyn Write + Send + Sync>) -> Self {
        Self {
            output: ShellOut::Write(AutoStream::never(output)),
        }
    }

    pub fn out(&mut self) -> &mut dyn Write {
        match &mut self.output {
            ShellOut::Streams { stdout, .. } => stdout,
            ShellOut::Write(output) => output,
        }
    }

    pub fn err(&mut self) -> &mut dyn Write {
        match &mut self.output {
            ShellOut::Streams { stderr, .. } => stderr,
            ShellOut::Write(output) => output,
        }
    }

    pub fn status(
        &mut self,
        status: impl fmt::Display,
        message: impl fmt::Display,
    ) -> io::Result<()> {
        self.message(&status, Some(&message), &STATUS, true)
    }

    pub fn error(&mut self, message: impl fmt::Display) -> io::Result<()> {
        self.message(&"error", Some(&message), &ERROR, false)
    }

    pub fn warn(&mut self, message: impl fmt::Display) -> io::Result<()> {
        self.message(&"warning", Some(&message), &WARNING, false)
    }

    pub fn note(&mut self, message: impl fmt::Display) -> io::Result<()> {
        self.message(&"note", Some(&message), &NOTE, false)
    }

    pub fn write_out_styled(
        &mut self,
        theme: ThemeStyle,
        message: impl fmt::Display,
    ) -> io::Result<()> {
        let style = theme.style();
        write!(self.out(), "{style}{message}{style:#}")
    }

    pub fn writeln_out_styled(
        &mut self,
        theme: ThemeStyle,
        message: impl fmt::Display,
    ) -> io::Result<()> {
        self.write_out_styled(theme, message)?;
        writeln!(self.out())
    }

    pub fn writeln_err_styled(
        &mut self,
        theme: ThemeStyle,
        message: impl fmt::Display,
    ) -> io::Result<()> {
        let style = theme.style();
        writeln!(self.err(), "{style}{message}{style:#}")
    }

    pub fn root_help(&mut self, help: &RootHelpV1) -> io::Result<()> {
        writeln!(self.out(), "{}\n", help.about)?;
        self.help_heading("Usage:")?;
        writeln!(self.out(), " {}\n", help.usage)?;
        if !help.commands.is_empty() {
            self.help_heading("Commands:")?;
            self.help_rows(&help.commands)?;
            writeln!(self.out())?;
        }
        if !help.options.is_empty() {
            self.help_heading("Options:")?;
            self.help_rows(&help.options)?;
        }
        Ok(())
    }

    fn message(
        &mut self,
        status: &dyn fmt::Display,
        message: Option<&dyn fmt::Display>,
        style: &Style,
        justified: bool,
    ) -> io::Result<()> {
        let mut buffer = Vec::new();
        if justified {
            write!(&mut buffer, "{style}{status:>12}{style:#}")?;
        } else {
            write!(&mut buffer, "{style}{status}{style:#}:")?;
        }
        match message {
            Some(message) => writeln!(&mut buffer, " {message}"),
            None => writeln!(&mut buffer),
        }?;
        self.err().write_all(&buffer)
    }

    fn help_heading(&mut self, heading: &str) -> io::Result<()> {
        writeln!(self.out(), "{HEADING}{heading}{HEADING:#}")
    }

    fn help_rows(&mut self, rows: &[HelpRow]) -> io::Result<()> {
        let width = rows.iter().map(|row| row.syntax.len()).max().unwrap_or(0);
        for row in rows {
            let mut lines = row.summary.lines();
            let summary = lines.next().unwrap_or_default();
            writeln!(
                self.out(),
                "  {LITERAL}{}{LITERAL:#}{}  {}",
                row.syntax,
                " ".repeat(width.saturating_sub(row.syntax.len())),
                summary
            )?;
            for line in lines {
                writeln!(self.out(), "{}{}", " ".repeat(width + 4), line)?;
            }
        }
        Ok(())
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);

    impl Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().write(bytes)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn diagnostics_and_status_share_one_format() {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let mut shell = Shell::from_write(Box::new(Buffer(bytes.clone())));
        shell.status("Checking", "project").unwrap();
        shell.warn("old toolchain").unwrap();
        shell.error("failed").unwrap();
        assert_eq!(
            String::from_utf8(bytes.lock().unwrap().clone()).unwrap(),
            "    Checking project\nwarning: old toolchain\nerror: failed\n"
        );
    }

    #[test]
    fn root_help_aligns_rows() {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let mut shell = Shell::from_write(Box::new(Buffer(bytes.clone())));
        shell
            .root_help(&RootHelpV1 {
                schema_version: ROOT_HELP_SCHEMA_V1.to_string(),
                name: "baml".to_string(),
                version: "1.0.0".to_string(),
                about: "BAML".to_string(),
                usage: "baml <COMMAND>".to_string(),
                commands: vec![
                    HelpRow {
                        syntax: "run".to_string(),
                        summary: "Run a function".to_string(),
                    },
                    HelpRow {
                        syntax: "toolchain".to_string(),
                        summary: "Manage toolchains".to_string(),
                    },
                ],
                options: vec![],
            })
            .unwrap();
        let actual = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        assert!(actual.contains("  run        Run a function\n"), "{actual}");
        assert!(
            actual.contains("  toolchain  Manage toolchains\n"),
            "{actual}"
        );
    }

    #[test]
    fn root_help_rejects_an_unknown_schema() {
        let help = RootHelpV1 {
            schema_version: "baml.root-help.v2".to_string(),
            name: "baml".to_string(),
            version: "2.0.0".to_string(),
            about: "BAML".to_string(),
            usage: "baml <COMMAND>".to_string(),
            commands: vec![],
            options: vec![],
        };
        assert_eq!(help.validate(), Err("unsupported root-help schema"));
    }
}
