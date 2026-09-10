//! `rename` — what F2 rewrites, and what it refuses to.
//!
//! A rename is only as good as the reference search behind it: an edit that
//! misses one occurrence silently breaks the program, which is worse than
//! offering no rename at all. So this module refuses about as much as it
//! renames, and every refusal is a MEASURED gap in [`crate::usages_at`]
//! rather than caution — see [`RenameError::IncompleteReferences`].
//!
//! The edit set is the declaration plus every reference, deduplicated:
//! `usages_at` includes an ITEM's declaration but not a member's or a
//! local's, so neither the union nor the dedup is optional.
//!
//! Every span is checked to spell the name being replaced before any edit
//! is handed back. A span that does not is a bug in the search, and a
//! rename is destructive enough that refusing the whole edit beats writing
//! one wrong byte.

use baml_base::SourceFile;
use baml_compiler_syntax::SyntaxKind;
use baml_compiler2_ppir::item_data::{self, MethodOwner};
use text_size::TextSize;

use crate::{
    resolve::{Location, SymbolTarget, symbol_at, target_definition},
    syntax::find_token_at_offset,
    usages::usages_at,
};

/// Why a rename cannot proceed.
///
/// Every variant reaches the reader as an editor message, so each one says
/// what is wrong rather than that something is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameError {
    /// The cursor is not on a name this server can resolve.
    NotASymbol,
    /// The declaration lives outside the workspace. Stdlib and dependency
    /// roots are read-only context, so their names cannot be rewritten.
    NotInWorkspace { name: String },
    /// [`crate::usages_at`] cannot yet find every reference to a symbol of
    /// this kind, so a rename would leave the program broken. The string
    /// names the kind, for a message the reader can act on.
    IncompleteReferences { kind: &'static str },
    /// The replacement is not one complete BAML identifier.
    InvalidName { name: String },
    /// A span the search returned does not spell the name being replaced.
    /// Refusing is the only safe answer: the alternative is an edit that
    /// corrupts unrelated source.
    SpanMismatch,
}

impl std::fmt::Display for RenameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotASymbol => f.write_str("there is no renameable symbol here"),
            Self::NotInWorkspace { name } => write!(
                f,
                "`{name}` is declared outside this project; \
                 standard-library and dependency sources are read-only"
            ),
            Self::IncompleteReferences { kind } => write!(
                f,
                "renaming {kind} is not supported yet: this server cannot yet \
                 find every reference to one, and a partial rename would \
                 leave your code broken"
            ),
            Self::InvalidName { name } => {
                write!(f, "`{name}` is not a valid BAML identifier")
            }
            Self::SpanMismatch => f.write_str(
                "internal error: the reference search returned a span that does \
                 not spell the name being renamed; no edit was made",
            ),
        }
    }
}

/// The identifier under the cursor, when it names something renameable.
///
/// This is `textDocument/prepareRename`: the editor uses the range to
/// highlight what is about to change and to pre-fill the input, and treats
/// an error as "F2 does nothing here". Every check [`rename`] makes about
/// the SYMBOL happens here too, so a rename never fails after the reader
/// has typed a new name — only the name itself can still be rejected.
pub fn prepare_rename(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
) -> Result<Location, RenameError> {
    renameable(db, file, offset).map(|target| target.cursor)
}

/// Every span to replace with `new_name`.
///
/// Regular function (not cached); the searches underneath are Salsa-cached.
pub fn rename(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
    new_name: &str,
) -> Result<Vec<Location>, RenameError> {
    let target = renameable(db, file, offset)?;
    if !baml_compiler_lexer::is_baml_identifier(new_name) {
        return Err(RenameError::InvalidName {
            name: new_name.to_string(),
        });
    }

    // `usages_at` includes an item's declaration but not a member's or a
    // local's, so the union is what makes the set complete and the dedup is
    // what keeps the declaration from being edited twice.
    let mut spans: Vec<Location> = Vec::new();
    for span in usages_at(db, file, offset)
        .into_iter()
        .chain(std::iter::once(target.declaration))
    {
        if !spans.contains(&span) {
            spans.push(span);
        }
    }

    // Fail closed: an edit is only safe if every span it rewrites is
    // currently the name being replaced.
    if !spans
        .iter()
        .all(|span| span_text(db, span) == Some(target.name.as_str()))
    {
        return Err(RenameError::SpanMismatch);
    }
    Ok(spans)
}

/// What a renameable position resolved to.
struct Renameable {
    /// The token under the cursor: what the editor highlights.
    cursor: Location,
    /// The name every edited span must currently spell.
    name: String,
    declaration: Location,
}

/// The one gate both entry points pass, so `prepareRename` and `rename`
/// can never disagree about what is renameable.
fn renameable(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
) -> Result<Renameable, RenameError> {
    let target = symbol_at(db, file, offset).ok_or(RenameError::NotASymbol)?;

    // The cursor must sit on the name itself — a rename replaces an
    // identifier, and nothing else is one.
    let token = find_token_at_offset(db, file, offset).ok_or(RenameError::NotASymbol)?;
    if token.kind() != SyntaxKind::WORD {
        return Err(RenameError::NotASymbol);
    }
    let cursor = Location {
        file,
        range: token.text_range(),
    };
    let name = token.text().to_string();

    let declaration = target_definition(db, target).ok_or(RenameError::NotASymbol)?;
    if declaration.file.source_root(db).kind(db) != baml_base::SourceRootKind::Workspace {
        return Err(RenameError::NotInWorkspace { name });
    }
    if let Some(kind) = references_are_incomplete(db, &target) {
        return Err(RenameError::IncompleteReferences { kind });
    }

    Ok(Renameable {
        cursor,
        name,
        declaration,
    })
}

/// The kinds whose reference search is known to miss occurrences, or `None`
/// when every reference is found.
///
/// Each entry is measured against a fixture that spells the symbol every way
/// the grammar allows, not assumed:
///
/// - An enum VARIANT is found in expression position (`Status.Active`) but
///   not in a match *type pattern* (`Status.Active =>`), because inference
///   records pattern types and no per-name pattern resolutions. This is the
///   gap [`crate::usages_at`]'s module doc names; closing it is a
///   compiler-side pattern-resolution record.
/// - An INTERFACE member and an `implements`-block method are found only
///   through their call sites: neither the interface's declaration, nor the
///   impl's, nor a sibling impl's is linked to the others here. Renaming
///   one would rewrite the calls and leave the declarations, or the
///   reverse. Closing it means walking `impls_naming_interface` and the
///   interface's own members, which is a search this module does not do.
///
/// A CLASS-inherent method is complete, which is why the owner is consulted
/// rather than the target kind alone.
fn references_are_incomplete(
    db: &dyn baml_compiler2_ppir::Db,
    target: &SymbolTarget<'_>,
) -> Option<&'static str> {
    match target {
        SymbolTarget::Item(_) | SymbolTarget::Local { .. } | SymbolTarget::Field { .. } => None,
        SymbolTarget::Method { func } => match item_data::method_owner(db, *func) {
            Some(MethodOwner::Class(_)) => None,
            Some(MethodOwner::Interface(_)) => Some("an interface method"),
            Some(MethodOwner::Impl(_)) => Some("a method of an `implements` block"),
            // A `Method` target always has an owner; without one there is
            // nothing to search from.
            None => Some("this method"),
        },
        SymbolTarget::Variant { .. } => Some("an enum variant"),
        SymbolTarget::InterfaceRequiredMethod { .. } => Some("an interface method"),
        SymbolTarget::InterfaceField { .. } => Some("an interface field"),
        SymbolTarget::AssociatedType { .. } => Some("an associated type"),
    }
}

/// The source text a span covers, or `None` when the span is not inside its
/// file (which a correct search never produces).
fn span_text<'db>(db: &'db dyn baml_compiler2_ppir::Db, span: &Location) -> Option<&'db str> {
    span.file
        .text(db)
        .get(std::ops::Range::<usize>::from(span.range))
}

#[cfg(test)]
mod tests {
    use super::{RenameError, prepare_rename, rename};
    use crate::test_support::CursorTest;

    /// Every kind of symbol, spelled every way the grammar allows, so the
    /// rename set can be compared against the source rather than assumed.
    const FIXTURE: &str = r#"enum Status {
    Active
    Done
}

class Box<T> {
    item: T
    tag: Status

    function get(self) -> T throws never { self.item }
}

interface Shows {
    function show(self) -> string throws never
}

implement Shows for Box<int> {
    function show(self) -> string throws never { "b" }
}

function make(tag: Status) -> Box<int> throws never {
    Box { item: 1, tag: tag }
}

function use_it(b: Box<int>, s: Status) -> string throws never {
    let inner: Box<int> = b;
    let g = inner.get();
    let t = inner.tag;
    let a = Status.Active;
    inner.show()
}
"#;

    /// A fixture cursor placed on the first `name` at or after `marker`.
    fn at(marker: &str, name: &str) -> CursorTest {
        let start = FIXTURE
            .find(marker)
            .unwrap_or_else(|| unreachable!("fixture contains {marker:?}"));
        let name_at = FIXTURE[start..]
            .find(name)
            .unwrap_or_else(|| unreachable!("{marker:?} is followed by {name:?}"))
            + start;
        CursorTest::new(&format!(
            "{}<[CURSOR]{}",
            &FIXTURE[..name_at],
            &FIXTURE[name_at..]
        ))
    }

    fn spans(test: &CursorTest, new_name: &str) -> Result<Vec<String>, RenameError> {
        rename(&test.db, test.cursor.file, test.cursor.offset, new_name).map(|spans| {
            spans
                .iter()
                .map(|span| test.format_file_range(span.file, span.range))
                .collect()
        })
    }

    /// The invariant the whole feature rests on: an edit only ever replaces
    /// text that currently spells the old name. A span covering a whole
    /// declaration rather than its name token would corrupt the file, so it
    /// is checked for every renameable kind rather than spot-checked.
    #[test]
    fn every_edited_span_spells_the_name_being_replaced() {
        for (marker, name) in [
            ("enum Status", "Status"),
            ("class Box<T>", "Box"),
            ("\n    item: T", "item"),
            ("function get(self)", "get"),
            ("interface Shows", "Shows"),
            ("function make(tag", "make"),
            ("let inner: Box<int>", "inner"),
            ("function use_it(b:", "b"),
            ("let g = inner.get", "g"),
        ] {
            let test = at(marker, name);
            let edits = rename(&test.db, test.cursor.file, test.cursor.offset, "renamed")
                .unwrap_or_else(|error| unreachable!("`{name}` is renameable: {error}"));
            assert!(!edits.is_empty(), "`{name}` has at least its declaration");
            let text = test.cursor.file.text(&test.db);
            for edit in &edits {
                assert_eq!(
                    &text[edit.range], name,
                    "a rename of `{name}` would have rewritten other source"
                );
            }
        }
    }

    #[test]
    fn an_item_rename_covers_its_declaration_and_every_use() {
        let test = at("enum Status", "Status");
        let edits = spans(&test, "State").expect("an enum is renameable");
        // Declaration, the field's type, both parameter types, and the
        // qualifier of `Status.Active`.
        assert_eq!(
            edits,
            vec![
                "test.baml:1:6",
                "test.baml:8:10",
                "test.baml:21:20",
                "test.baml:25:33",
                "test.baml:29:13",
            ],
            "got {edits:?}"
        );
    }

    #[test]
    fn a_member_rename_adds_the_declaration_the_search_leaves_out() {
        // `usages_at` returns a member's uses but not its declaration, so
        // the union is what makes this complete.
        let test = at("\n    item: T", "item");
        let edits = spans(&test, "value").expect("a class field is renameable");
        assert!(
            edits.contains(&"test.baml:7:5".to_string()),
            "the field's own declaration is edited: {edits:?}"
        );
        assert!(
            edits.contains(&"test.baml:10:49".to_string())
                && edits.contains(&"test.baml:22:11".to_string()),
            "`self.item` and the constructor key are edited: {edits:?}"
        );
    }

    #[test]
    fn a_local_rename_stays_inside_its_function() {
        let test = at("let inner: Box<int>", "inner");
        let edits = spans(&test, "boxed").expect("a local is renameable");
        assert_eq!(edits.len(), 4, "declaration plus three uses: {edits:?}");
    }

    /// The refusals. Each is a measured gap in the reference search, so a
    /// rename that silently half-applied is what these prevent.
    #[test]
    fn kinds_whose_references_are_incomplete_are_refused() {
        for (marker, name, expected) in [
            ("\n    Active", "Active", "an enum variant"),
            (
                "    function show(self) -> string throws never\n",
                "show",
                "an interface method",
            ),
            (
                "function show(self) -> string throws never { \"b\" }",
                "show",
                "a method of an `implements` block",
            ),
        ] {
            let test = at(marker, name);
            let error = rename(&test.db, test.cursor.file, test.cursor.offset, "other")
                .expect_err("the reference search cannot cover this yet");
            assert_eq!(
                error,
                RenameError::IncompleteReferences { kind: expected },
                "renaming `{name}`"
            );
            // `prepareRename` refuses too, so the editor greys F2 out rather
            // than failing after the reader has typed a new name.
            assert!(prepare_rename(&test.db, test.cursor.file, test.cursor.offset).is_err());
        }
    }

    #[test]
    fn the_standard_library_is_read_only() {
        let test = at("let a = Status.Active", "Status");
        // A workspace symbol is fine; a stdlib one is not.
        assert!(rename(&test.db, test.cursor.file, test.cursor.offset, "State").is_ok());

        let stdlib = CursorTest::new(
            "function f() -> int throws never {\n    let d = baml.time.Dur<[CURSOR]ation;\n    0\n}\n",
        );
        let error = rename(&stdlib.db, stdlib.cursor.file, stdlib.cursor.offset, "Span")
            .expect_err("stdlib sources are read-only");
        assert!(
            matches!(error, RenameError::NotInWorkspace { .. }),
            "got {error:?}"
        );
    }

    #[test]
    fn the_replacement_must_be_one_identifier() {
        let test = at("enum Status", "Status");
        for bad in ["", "2legit", "has space", "with.dot", "class", "type"] {
            let error = rename(&test.db, test.cursor.file, test.cursor.offset, bad)
                .expect_err("`{bad}` is not an identifier");
            assert_eq!(
                error,
                RenameError::InvalidName {
                    name: bad.to_string()
                },
                "renaming to {bad:?}"
            );
        }
    }

    #[test]
    fn prepare_rename_returns_the_token_under_the_cursor() {
        let test = at("let a = Status.Active", "Status");
        let range = prepare_rename(&test.db, test.cursor.file, test.cursor.offset)
            .expect("a use site is renameable");
        let text = test.cursor.file.text(&test.db);
        assert_eq!(&text[range.range], "Status");
    }

    #[test]
    fn a_position_that_names_nothing_is_refused() {
        let test = CursorTest::new("function f() -> int throws never {\n    <[CURSOR]42\n}\n");
        assert_eq!(
            prepare_rename(&test.db, test.cursor.file, test.cursor.offset),
            Err(RenameError::NotASymbol)
        );
    }
}
