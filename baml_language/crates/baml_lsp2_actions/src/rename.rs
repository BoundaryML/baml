//! Compiler-owned prepare-rename and rename spans.

use baml_base::SourceFile;
use baml_compiler_syntax::SyntaxKind;
use text_size::TextSize;

use crate::{Db, Location, definition_at, search_symbols, usages_at, utils};

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
    let token = utils::find_token_at_offset(db, file, offset).ok_or(RenameError::NotRenameable)?;
    if token.kind() != SyntaxKind::WORD || !is_identifier(token.text()) {
        return Err(RenameError::NotRenameable);
    }
    let definition = definition_at(db, file, offset).ok_or(RenameError::NotRenameable)?;
    let name = token.text().to_string();
    let name_len = TextSize::try_from(name.len()).map_err(|_| RenameError::SpanMismatch)?;
    if definition.range.len() != name_len {
        return Err(RenameError::SpanMismatch);
    }
    Ok(RenameTarget { name, definition })
}

pub fn rename(
    db: &dyn Db,
    file: SourceFile,
    offset: TextSize,
    new_name: &str,
) -> Result<Vec<Location>, RenameError> {
    if !is_identifier(new_name) {
        return Err(RenameError::InvalidIdentifier(new_name.to_string()));
    }
    let target = prepare_rename(db, file, offset)?;
    if new_name != target.name
        && search_symbols(db, &crate::usages::collect_source_files(db, file), new_name)
            .iter()
            .any(|symbol| {
                symbol.name == new_name
                    && (symbol.file != target.definition.file
                        || symbol.name_span != target.definition.range)
            })
    {
        return Err(RenameError::Collision(new_name.to_string()));
    }

    let mut locations = usages_at(db, file, offset);
    locations.push(target.definition);
    locations.sort_by_key(|location| (location.file.path(db), location.range.start()));
    locations.dedup();
    if locations.iter().any(|location| {
        let range = std::ops::Range::<usize>::from(location.range);
        location.file.text(db).get(range) != Some(target.name.as_str())
    }) {
        return Err(RenameError::SpanMismatch);
    }
    Ok(locations)
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::CursorTest;

    #[test]
    fn rename_returns_definition_and_references() {
        let test =
            CursorTest::new("class Per<[CURSOR]son {}\nfunction Use(p: Person) -> Person { p }\n");
        let locations = rename(&test.db, test.cursor.file, test.cursor.offset, "Human").unwrap();
        assert_eq!(locations.len(), 3);
    }

    #[test]
    fn rename_rejects_invalid_identifier() {
        let test = CursorTest::new("class Per<[CURSOR]son {}\n");
        assert!(matches!(
            rename(&test.db, test.cursor.file, test.cursor.offset, "not-valid"),
            Err(RenameError::InvalidIdentifier(_))
        ));
    }

    #[test]
    fn rename_rejects_project_wide_collision() {
        let test = CursorTest::new("class Per<[CURSOR]son {}\nclass Human {}\n");
        assert!(matches!(
            rename(&test.db, test.cursor.file, test.cursor.offset, "Human"),
            Err(RenameError::Collision(_))
        ));
    }
}
