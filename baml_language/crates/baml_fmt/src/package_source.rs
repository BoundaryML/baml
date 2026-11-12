//! A package's source as it is recorded and hashed (BEP-088): every file
//! formatted and normalized, so the same code is the same bytes, and the
//! same hash, on every machine and under every checkout style.

use std::{collections::BTreeMap, path::Path};

use baml_db::SourceRoot;
use unicode_normalization::UnicodeNormalization;

use crate::{FormatOptions, format_salsa};

const BYTE_ORDER_MARK: char = '\u{feff}';

/// A package's formatted, normalized source text: what its source hash is
/// taken over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageSource {
    /// Normalized path (package-relative, `/`-separated, NFC, case-folded) to
    /// normalized text.
    pub files: BTreeMap<String, String>,
}

/// Why a package's source has no normalized form.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PackageSourceError {
    /// Two files have one normalized path: they differ only by letter case
    /// or by Unicode composition, and a case-insensitive filesystem holds
    /// only one of them.
    #[error("`{first}` and `{second}` are the same path once case and Unicode form are normalized")]
    PathCollision { first: String, second: String },
    /// A path that is not Unicode has no portable spelling.
    #[error("`{path}` is not a Unicode path")]
    NonUnicodePath { path: String },
}

/// `root`'s source, formatted and normalized.
///
/// Each file is formatted with the canonical options as the database holds
/// it; nothing on disk changes. A file the formatter leaves alone (one that
/// opts out with `baml-format: ignore`, or does not parse) is taken as
/// written. The database already holds text with `\n` line breaks; a leading
/// byte-order mark is dropped.
///
/// # Errors
///
/// [`PackageSourceError`]: two files share a normalized path, or a path is
/// not Unicode.
pub fn package_source(
    db: &dyn salsa::Database,
    root: SourceRoot,
) -> Result<PackageSource, PackageSourceError> {
    let root_path = root.path(db);
    let mut files = BTreeMap::new();
    let mut written: BTreeMap<String, String> = BTreeMap::new();
    for &file in root.files(db) {
        let path = file.path(db);
        let relative = path.strip_prefix(root_path).unwrap_or(&path);
        let (normalized, spelled) = normalize_path(relative)?;
        if let Some(first) = written.insert(normalized.clone(), spelled.clone()) {
            return Err(PackageSourceError::PathCollision {
                first,
                second: spelled,
            });
        }
        let text = file.text(db);
        // The lexer does not read a byte-order mark as whitespace, so a file
        // that opens with one is formatted without it.
        let formatted = match text.strip_prefix(BYTE_ORDER_MARK) {
            None => format_salsa(db, file, FormatOptions::default())
                .map_or_else(|_| text.clone(), |formatted| formatted.apply_to(text)),
            Some(unmarked) => crate::format(unmarked, &FormatOptions::default())
                .unwrap_or_else(|_| unmarked.to_owned()),
        };
        files.insert(normalized, normalize_text(&formatted));
    }
    Ok(PackageSource { files })
}

/// A package-relative path as it is recorded: components joined with `/`, in
/// Unicode NFC form, case-folded. Returns it with the path as written, for
/// diagnostics.
fn normalize_path(relative: &Path) -> Result<(String, String), PackageSourceError> {
    let mut components = Vec::new();
    for component in relative.components() {
        let component =
            component
                .as_os_str()
                .to_str()
                .ok_or_else(|| PackageSourceError::NonUnicodePath {
                    path: relative.to_string_lossy().into_owned(),
                })?;
        components.push(component);
    }
    let spelled = components.join("/");
    let folded = caseless::default_case_fold_str(&spelled.nfc().collect::<String>());
    Ok((folded.nfc().collect(), spelled))
}

/// Formatted text as it is recorded: `\n` line breaks and no byte-order mark.
fn normalize_text(text: &str) -> String {
    let text = text.strip_prefix(BYTE_ORDER_MARK).unwrap_or(text);
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use baml_db::{ProjectDatabase, SourceRootKind, SourceRootSpec};

    use super::*;

    fn package(files: &[(&str, &str)]) -> Result<PackageSource, PackageSourceError> {
        let mut db = ProjectDatabase::new();
        let root = db
            .add_source_root(SourceRootSpec::new("<pkg>", SourceRootKind::Workspace))
            .unwrap();
        for (path, text) in files {
            db.add_or_update_file_in(root, &PathBuf::from("<pkg>").join(path), text);
        }
        package_source(&db, root)
    }

    const FORMATTED: &str = "function f() -> int {\n    1\n}\n";

    #[test]
    fn formatting_line_endings_and_bom_do_not_change_the_source() {
        let base = package(&[("a.baml", FORMATTED)]).unwrap();
        assert_eq!(base.files["a.baml"], FORMATTED);
        for variant in [
            "function f() -> int { 1 }",
            "function f()   ->   int {\r\n  1\r\n}\r\n",
            "\u{feff}function f() -> int {\n    1\n}\n",
            "\u{feff}function f() -> int { 1 }",
        ] {
            let other = package(&[("a.baml", variant)]).unwrap();
            assert_eq!(other, base, "{variant:?}");
        }
    }

    #[test]
    fn edits_that_survive_formatting_change_the_source() {
        let base = package(&[("a.baml", FORMATTED)]).unwrap();
        let commented = package(&[("a.baml", "// why\nfunction f() -> int {\n    1\n}\n")]);
        assert_ne!(commented.unwrap(), base);
        let body = package(&[("a.baml", "function f() -> int {\n    2\n}\n")]);
        assert_ne!(body.unwrap(), base);
        let moved = package(&[("b.baml", FORMATTED)]);
        assert_ne!(moved.unwrap(), base);
    }

    #[test]
    fn a_file_the_formatter_leaves_alone_is_taken_as_written() {
        let ignored = "// baml-format: ignore\nfunction f()->int{1}\n";
        assert_eq!(
            package(&[("a.baml", ignored)]).unwrap().files["a.baml"],
            ignored
        );
        let broken = "function f( -> {\n";
        assert_eq!(
            package(&[("a.baml", broken)]).unwrap().files["a.baml"],
            broken
        );
    }

    #[test]
    fn paths_are_relative_slashed_nfc_and_case_folded() {
        // "É" precomposed, and as "E" with a combining acute accent.
        let composed = package(&[("ns_Caf\u{c9}/Main.baml", FORMATTED)]).unwrap();
        let decomposed = package(&[("ns_CafE\u{301}/Main.baml", FORMATTED)]).unwrap();
        assert_eq!(
            composed.files.keys().collect::<Vec<_>>(),
            ["ns_caf\u{e9}/main.baml"]
        );
        assert_eq!(composed, decomposed);
    }

    #[test]
    fn paths_that_normalize_alike_collide() {
        assert_eq!(
            package(&[("Main.baml", FORMATTED), ("main.baml", "")]),
            Err(PackageSourceError::PathCollision {
                first: "Main.baml".to_owned(),
                second: "main.baml".to_owned(),
            })
        );
    }
}
