//! Compiler-owned prepare-rename and rename spans.

use baml_base::SourceFile;
use baml_compiler_syntax::SyntaxKind;
use baml_compiler2_hir::{contributions::DefinitionKind, file_package::file_package};
use text_size::TextSize;

use crate::{Db, Location, definition_at, search::SymbolInfo, search_symbols, usages_at, utils};

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
    // A rename emits the new name verbatim into source, so a reserved word
    // would immediately produce unparsable BAML.
    if is_reserved_word(new_name) {
        return Err(RenameError::ReservedWord(new_name.to_string()));
    }
    let target = prepare_rename(db, file, offset)?;
    if new_name != target.name {
        let files = crate::usages::collect_source_files(db, file);
        let target_info = search_symbols(db, &files, &target.name)
            .into_iter()
            .find(|symbol| {
                symbol.file == target.definition.file && symbol.name_span == target.definition.range
            });
        let collides = |symbol: &SymbolInfo| {
            symbol.name == new_name
                && (symbol.file != target.definition.file
                    || symbol.name_span != target.definition.range)
        };
        let collision = match &target_info {
            // The target's scope is known, so only a same-scope, same-lookup-
            // space spelling collides: members collide within their own
            // container, and top-level items collide within their own
            // namespace and namespace kind (BAML's type and value spaces can
            // legitimately share a spelling).
            Some(target_info) => search_symbols(db, &files, new_name)
                .iter()
                .any(|symbol| collides(symbol) && same_scope(db, symbol, target_info)),
            // The target's own symbol info is unavailable; stay conservative
            // and reject any same-spelling symbol project-wide.
            None => search_symbols(db, &files, new_name).iter().any(collides),
        };
        if collision {
            return Err(RenameError::Collision(new_name.to_string()));
        }
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

fn same_scope(db: &dyn Db, candidate: &SymbolInfo, target: &SymbolInfo) -> bool {
    if candidate.container_name != target.container_name {
        return false;
    }
    if candidate.container_name.is_none()
        && is_type_space(candidate.kind) != is_type_space(target.kind)
    {
        return false;
    }
    namespace_of(db, candidate.file) == namespace_of(db, target.file)
}

fn namespace_of(db: &dyn Db, file: SourceFile) -> Vec<String> {
    file_package(db, file)
        .namespace_path
        .iter()
        .map(|segment| segment.as_str().to_string())
        .collect()
}

fn is_type_space(kind: DefinitionKind) -> bool {
    matches!(
        kind,
        DefinitionKind::Class
            | DefinitionKind::Enum
            | DefinitionKind::Interface
            | DefinitionKind::TypeAlias
    )
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_alphanumeric())
}

/// Words the lexer never produces as `WORD` tokens, mirroring
/// `SyntaxKind::is_keyword`. Renaming to one would emit unparsable BAML.
fn is_reserved_word(name: &str) -> bool {
    matches!(
        name,
        "class"
            | "enum"
            | "interface"
            | "implements"
            | "implement"
            | "extends"
            | "requires"
            | "function"
            | "client"
            | "generator"
            | "test"
            | "testset"
            | "retry_policy"
            | "template_string"
            | "type_builder"
            | "if"
            | "else"
            | "for"
            | "while"
            | "let"
            | "const"
            | "in"
            | "is"
            | "break"
            | "continue"
            | "return"
            | "throw"
            | "match"
            | "catch"
            | "catch_all"
            | "catch_all_panics"
            | "throws"
            | "spawn"
            | "await"
            | "defer"
            | "instanceof"
            | "dynamic"
            | "with"
            | "as"
            | "type"
    )
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
    fn rename_rejects_reserved_words() {
        let test = CursorTest::new("class Per<[CURSOR]son {}\n");
        for reserved in ["class", "function", "enum", "type"] {
            assert!(
                matches!(
                    rename(&test.db, test.cursor.file, test.cursor.offset, reserved),
                    Err(RenameError::ReservedWord(_))
                ),
                "reserved word {reserved} must be rejected"
            );
        }
    }

    #[test]
    fn rename_rejects_same_scope_collision() {
        let test = CursorTest::new("class Per<[CURSOR]son {}\nclass Human {}\n");
        assert!(matches!(
            rename(&test.db, test.cursor.file, test.cursor.offset, "Human"),
            Err(RenameError::Collision(_))
        ));
    }

    #[test]
    fn rename_allows_type_and_value_spaces_to_share_a_spelling() {
        // `class Foo` and `function Foo` may coexist, so renaming a function
        // to a class's spelling is legal.
        let test = CursorTest::new("class Human {}\nfunction Gr<[CURSOR]eet() -> int { 1 }\n");
        let result = rename(&test.db, test.cursor.file, test.cursor.offset, "Human");
        assert!(
            result.is_ok(),
            "value-space rename failed: {}",
            result.err().map(|e| e.to_string()).unwrap_or_default()
        );
    }

    #[test]
    fn rename_allows_member_spelling_used_by_unrelated_top_level_symbol() {
        // A class field lives in its container's scope; a top-level symbol
        // with the same spelling must not block the rename.
        let test = CursorTest::new("class Person { na<[CURSOR]me string }\nclass Name {}\n");
        let result = rename(&test.db, test.cursor.file, test.cursor.offset, "Name");
        assert!(
            result.is_ok(),
            "member rename failed: {}",
            result.err().map(|e| e.to_string()).unwrap_or_default()
        );
    }

    #[test]
    fn rename_rejects_member_collision_within_the_same_container() {
        let test = CursorTest::new("class Person { na<[CURSOR]me string\n  Name string }\n");
        assert!(matches!(
            rename(&test.db, test.cursor.file, test.cursor.offset, "Name"),
            Err(RenameError::Collision(_))
        ));
    }
}
