//! Left-aligned text columns, each as wide as its widest cell.

use std::io::{self, Write};

pub(crate) fn widths(header: &[String], rows: &[Vec<String>]) -> Vec<usize> {
    let mut widths: Vec<usize> = header.iter().map(|cell| cell.chars().count()).collect();
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    widths
}

pub(crate) fn line(
    out: &mut dyn Write,
    cells: &[String],
    widths: &[usize],
    separator: &str,
) -> io::Result<()> {
    let parts: Vec<String> = cells
        .iter()
        .zip(widths)
        .map(|(cell, width)| format!("{cell:<width$}"))
        .collect();
    writeln!(out, "{}", parts.join(separator).trim_end())
}

/// A header row, then one row per item.
pub(crate) fn write(out: &mut dyn Write, header: &[&str], rows: &[Vec<String>]) -> io::Result<()> {
    let header: Vec<String> = header.iter().map(|cell| (*cell).to_owned()).collect();
    let widths = widths(&header, rows);
    line(out, &header, &widths, "  ")?;
    for row in rows {
        line(out, row, &widths, "  ")?;
    }
    Ok(())
}
