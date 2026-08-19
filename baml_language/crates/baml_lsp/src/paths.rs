//! The URI ↔ database-path boundary.
//!
//! Every path that enters the owner state from the wire passes through
//! [`canonical_document_path`] exactly once, and every database path that
//! leaves it passes through [`uri_for_db_path`]. Two rules compose:
//!
//! - **Physical identity.** [`canonical_physical_path`] resolves URI aliases
//!   (`/tmp/x` vs `/private/tmp/x`, `a/../b`) to one spelling with the same
//!   rule `baml_db` applies to its own keys: canonicalize the longest
//!   *existing* ancestor and re-append the rest verbatim, so a not-yet-saved
//!   buffer under a symlinked directory shares its root's prefix.
//! - **Stdlib mapping.** The database stores stdlib files under the virtual
//!   `<builtin>/<pkg>/…` prefix (a wire contract shared with emitted
//!   bytecode); when the host materialized the stubs on disk,
//!   [`crate::roots::RootsView`] swaps that prefix for the directory in both
//!   directions.
//!
//! The Windows lowercase folding the previous server applied is deliberately
//! absent: the database does not fold case, and the owner's root index must
//! agree with the database's keys byte for byte.

#[cfg(target_arch = "wasm32")]
use std::path::Component;
use std::path::{Path, PathBuf};

use lsp_types::Url;

use crate::{error::LspError, roots::RootsView};

/// One physical identity for a filesystem path.
///
/// Lexically drops `.` and resolves `..`, then canonicalizes the longest
/// existing ancestor and re-appends the missing tail verbatim. A path with no
/// existing ancestor (virtual paths, everything on wasm) is returned lexically
/// normalized. Relative paths are normalized lexically only.
pub fn canonical_physical_path(path: &Path) -> PathBuf {
    #[cfg(not(target_arch = "wasm32"))]
    {
        // The database's own rule (`baml_db::canonicalize_lossy`), so a path
        // derived here equals the key the database stores it under byte for
        // byte.
        baml_db::canonicalize_lossy(path)
    }
    #[cfg(target_arch = "wasm32")]
    {
        // No filesystem: lexical normalization is the whole rule (matching
        // the database's no-existing-ancestor branch).
        lexically_normalize(path)
    }
}

#[cfg(target_arch = "wasm32")]
fn lexically_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Popping a root or prefix is a no-op, matching `..` at `/`.
                out.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                out.push(component.as_os_str());
            }
        }
    }
    out
}

/// The database path for a document URI: physical identity, then the stdlib
/// mapping. Non-`file:` URIs are `InvalidPath`.
pub fn canonical_document_path(roots: &RootsView, uri: &Url) -> Result<PathBuf, LspError> {
    let path = url_to_file_path(uri).ok_or_else(|| LspError::InvalidPath {
        path: PathBuf::from(uri.as_str()),
        message: "not a file URI".to_owned(),
    })?;
    Ok(roots.to_db_path(&canonical_physical_path(&path)))
}

/// The URI a client can open for a database path. `None` when the path has
/// no presentation (a stdlib file with no materialized directory) or cannot
/// be spelled as a `file:` URL.
pub fn uri_for_db_path(roots: &RootsView, db_path: &Path) -> Option<Url> {
    let presentation = roots.to_presentation_path(db_path)?;
    file_path_to_url(&presentation)
}

#[cfg(not(target_arch = "wasm32"))]
fn url_to_file_path(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn file_path_to_url(path: &Path) -> Option<Url> {
    Url::from_file_path(path).ok()
}

/// `url` gates `to_file_path` on unix/windows; the browser build decodes the
/// percent-encoded path itself. Paths there are virtual and always
/// `/`-rooted.
#[cfg(target_arch = "wasm32")]
fn url_to_file_path(uri: &Url) -> Option<PathBuf> {
    if uri.scheme() != "file" {
        return None;
    }
    let decoded = percent_encoding::percent_decode_str(uri.path())
        .decode_utf8()
        .ok()?;
    Some(PathBuf::from(decoded.as_ref()))
}

#[cfg(target_arch = "wasm32")]
fn file_path_to_url(path: &Path) -> Option<Url> {
    use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
    /// Everything a URL path segment must escape.
    const PATH_SEGMENT: &AsciiSet = &CONTROLS
        .add(b' ')
        .add(b'"')
        .add(b'#')
        .add(b'%')
        .add(b'/')
        .add(b'<')
        .add(b'>')
        .add(b'?')
        .add(b'`')
        .add(b'{')
        .add(b'}');
    let mut serialization = String::from("file://");
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(segment) => {
                serialization.push('/');
                serialization
                    .push_str(&utf8_percent_encode(segment.to_str()?, PATH_SEGMENT).to_string());
            }
            Component::CurDir | Component::ParentDir | Component::Prefix(_) => return None,
        }
    }
    if serialization.len() == "file://".len() {
        serialization.push('/');
    }
    Url::parse(&serialization).ok()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn roots(stdlib_dir: Option<PathBuf>) -> Arc<RootsView> {
        RootsView::new(Vec::new(), stdlib_dir)
    }

    #[test]
    fn lexical_normalization_matches_the_database_rule() {
        // Non-existing inputs take the lexical branch of the shared rule.
        assert_eq!(
            canonical_physical_path(Path::new("/nonexistent-A7/x/./b/../c")),
            PathBuf::from("/nonexistent-A7/x/c"),
        );
        assert_eq!(
            canonical_physical_path(Path::new("/../nonexistent-A7")),
            PathBuf::from("/nonexistent-A7"),
        );
    }

    #[test]
    fn missing_tail_is_appended_to_the_canonical_existing_ancestor() {
        let temp = tempfile::tempdir().unwrap();
        let canonical_root = temp.path().canonicalize().unwrap();
        let missing = temp.path().join("not").join("yet.baml");
        assert_eq!(
            canonical_physical_path(&missing),
            canonical_root.join("not").join("yet.baml")
        );
    }

    #[test]
    fn existing_path_is_fully_canonical() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("a.baml");
        std::fs::write(&file, "").unwrap();
        assert_eq!(canonical_physical_path(&file), file.canonicalize().unwrap());
    }

    #[test]
    fn document_path_rejects_non_file_uris() {
        let uri = Url::parse("untitled:Untitled-1").unwrap();
        assert!(matches!(
            canonical_document_path(&roots(None), &uri),
            Err(LspError::InvalidPath { .. })
        ));
    }

    #[test]
    fn stdlib_round_trip_through_a_materialized_directory() {
        let temp = tempfile::tempdir().unwrap();
        // The view stores the canonical directory, as the owner does.
        let stdlib_dir = temp.path().canonicalize().unwrap();
        let roots = roots(Some(stdlib_dir.clone()));

        let db_path = Path::new("<builtin>/std/prelude.baml");
        let uri = uri_for_db_path(&roots, db_path).expect("materialized stdlib has a URI");
        assert_eq!(
            uri.to_file_path().unwrap(),
            stdlib_dir.join("std").join("prelude.baml")
        );
        assert_eq!(canonical_document_path(&roots, &uri).unwrap(), db_path);
    }

    #[test]
    fn stdlib_without_a_directory_has_no_uri() {
        assert!(uri_for_db_path(&roots(None), Path::new("<builtin>/std/prelude.baml")).is_none());
    }

    #[test]
    fn workspace_paths_round_trip_unchanged() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().canonicalize().unwrap().join("main.baml");
        let roots = roots(None);
        let uri = uri_for_db_path(&roots, &file).unwrap();
        assert_eq!(canonical_document_path(&roots, &uri).unwrap(), file);
    }
}
