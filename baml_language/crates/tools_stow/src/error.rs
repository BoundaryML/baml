//! Every way a run can fail before or outside validation (spec section 10).

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error("cannot determine the current directory: {0}")]
    CurrentDir(#[source] std::io::Error),

    #[error("no Cargo.toml found in {} or any parent directory", .0.display())]
    NoManifest(PathBuf),

    #[error("cargo metadata failed: {0}")]
    Metadata(#[from] cargo_metadata::Error),

    #[error(
        "no stow.toml and no [workspace.metadata.stow] in {}; run `cargo stow init`",
        .0.display()
    )]
    NoConfig(PathBuf),

    #[error("{} already exists", .0.display())]
    ConfigExists(PathBuf),

    #[error("cannot read {}: {source}", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot write {}: {source}", .path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot parse {}: {message}", .path.display())]
    Parse { path: PathBuf, message: String },

    #[error("graph nodes `{first}` and `{second}` share the id `{id}`")]
    DuplicateNodeId {
        id: String,
        first: String,
        second: String,
    },
}

pub(crate) type Result<T> = std::result::Result<T, Error>;
