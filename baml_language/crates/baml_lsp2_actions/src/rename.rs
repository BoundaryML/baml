//! Compiler-owned prepare-rename and rename spans (API stub).
//!
//! Signatures are final; the collision-validated implementation lands in the
//! rename-actions commit together with its tests.

use baml_base::SourceFile;
use text_size::TextSize;

use crate::{Db, Location};

#[derive(Clone, PartialEq, Eq)]
pub struct RenameTarget {
    pub name: String,
    pub definition: Location,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenameError {
    #[error("the cursor is not on a renameable BAML identifier")]
    NotRenameable,
    #[error("{0:?} is not a valid BAML identifier")]
    InvalidIdentifier(String),
    #[error("{0:?} is a reserved BAML word")]
    ReservedWord(String),
    #[error("renaming to {0:?} would collide with an existing symbol")]
    Collision(String),
    #[error("a compiler span failed the rename round-trip check")]
    SpanMismatch,
}

pub fn prepare_rename(
    db: &dyn Db,
    file: SourceFile,
    offset: TextSize,
) -> Result<RenameTarget, RenameError> {
    let _ = (db, file, offset);
    todo!("implemented in the rename-actions commit")
}

pub fn rename(
    db: &dyn Db,
    file: SourceFile,
    offset: TextSize,
    new_name: &str,
) -> Result<Vec<Location>, RenameError> {
    let _ = (db, file, offset, new_name);
    todo!("implemented in the rename-actions commit")
}
