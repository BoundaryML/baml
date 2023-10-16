use crate::Span;
use colored::{ColoredString, Colorize};

pub trait DiagnosticColorer {
    fn title(&self) -> &'static str;

    fn primary_color(&self, token: &'_ str) -> ColoredString;
}

/// Given the datamodel text representation, pretty prints an error or warning, including
/// the offending portion of the source code, for human-friendly reading.
pub(crate) fn pretty_print(
    f: &mut dyn std::io::Write,
    file_name: &str,
    text: &str,
    span: Span,
    description: &str,
    colorer: &'static dyn DiagnosticColorer,
) -> std::io::Result<()> {
    let start_line_number = text[..span.start].matches('\n').count();
    let end_line_number = text[..span.end].matches('\n').count();
    let file_lines = text.split('\n').collect::<Vec<&str>>();

    let chars_in_line_before: usize = file_lines[..start_line_number].iter().map(|l| l.len()).sum();
    // Don't forget to count the all the line breaks.
    let chars_in_line_before = chars_in_line_before + start_line_number;

    let line = &file_lines[start_line_number];

    let start_in_line = span.start - chars_in_line_before;
    let end_in_line = std::cmp::min(start_in_line + (span.end - span.start), line.len());

    let prefix = &line[..start_in_line];
    let offending = colorer.primary_color(&line[start_in_line..end_in_line]).bold();
    let suffix = &line[end_in_line..];

    let arrow = "-->".bright_blue().bold();
    let file_path = format!("{}:{}", file_name, start_line_number + 1).underline();

    writeln!(
        f,
        "{}: {}",
        colorer.primary_color(colorer.title()).bold(),
        description.bold()
    )?;
    writeln!(f, "  {arrow}  {file_path}")?;
    writeln!(f, "{}", format_line_number(0))?;

    writeln!(f, "{}", format_line_number_with_line(start_line_number, &file_lines))?;
    writeln!(
        f,
        "{}{}{}{}",
        format_line_number(start_line_number + 1),
        prefix,
        offending,
        suffix
    )?;
    if offending.len() == 0 {
        let spacing = " ".repeat(start_in_line);
        writeln!(
            f,
            "{}{}{}",
            format_line_number(0),
            spacing,
            colorer.primary_color("^ Unexpected token.").bold()
        )?;
    }

    for line_number in start_line_number + 2..end_line_number + 2 {
        writeln!(f, "{}", format_line_number_with_line(line_number, &file_lines))?;
    }

    writeln!(f, "{}", format_line_number(0))
}

fn format_line_number_with_line(line_number: usize, lines: &[&str]) -> colored::ColoredString {
    if line_number > 0 && line_number <= lines.len() {
        colored::ColoredString::from(format!("{}{}", format_line_number(line_number), lines[line_number - 1]).as_str())
    } else {
        format_line_number(line_number)
    }
}

fn format_line_number(line_number: usize) -> colored::ColoredString {
    if line_number > 0 {
        format!("{line_number:2} | ").bold().bright_blue()
    } else {
        "   | ".bold().bright_blue()
    }
}
