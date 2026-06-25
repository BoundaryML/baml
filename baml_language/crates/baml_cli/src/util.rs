//! Small formatting helpers shared across CLI subcommands.

use std::path::{Path, PathBuf};

/// 1-based line number at a byte `offset` within `text`.
///
/// Counts newline *bytes* rather than slicing the `str`, so an `offset` that
/// lands mid-codepoint (offsets are derived from byte-level scanning) can't
/// panic.
pub(crate) fn line_number_at_offset(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    text.as_bytes()[..offset]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

/// Make `path` relative to `root`, or return it unchanged if not under `root`.
pub(crate) fn relative_path(path: &Path, root: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}
