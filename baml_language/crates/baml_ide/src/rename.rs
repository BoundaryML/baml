//! `rename` — what F2 rewrites, and what it refuses to.
//!
//! A rename is only as good as the reference search behind it: an edit that
//! misses one occurrence silently breaks the program, which is worse than
//! offering no rename at all. So this module refuses about as much as it
//! renames, and every refusal is a MEASURED gap in [`crate::usages_at`]
//! rather than caution — see [`RenameError::IncompleteReferences`].
//!
//! The edit set is every declaration the language forces to share one
//! spelling, plus every reference to each of them, deduplicated. For most
//! symbols the first list has one entry; for an interface member it has
//! one per `implements` block, because the compiler pairs an impl's members
//! with the interface's BY NAME and rejects a mismatch outright
//! (`MissingInterfaceMethod`, `UnknownInterfaceMember`,
//! `MissingInterfaceField`). See [`rename_group`].
//!
//! The union and the dedup are both load-bearing: `usages_at` includes an
//! ITEM's declaration but not a member's or a local's, and two group
//! members can report the same span.
//!
//! Every span is checked to spell the name being replaced before any edit
//! is handed back. A span that does not is a bug in the search, and a
//! rename is destructive enough that refusing the whole edit beats writing
//! one wrong byte.
//!
//! The claim under all of it — that an accepted rename leaves a program
//! that still compiles — is tested by EXECUTION: the tests apply the edits
//! and run the checker over the result, so a group missing a coupled
//! declaration fails with the compiler's own diagnostic rather than with a
//! span count nobody can read.

use baml_base::{Name, SourceFile};
use baml_compiler_syntax::SyntaxKind;
use baml_compiler2_hir::loc::{ClassLoc, ImplLoc, InterfaceLoc};
use baml_compiler2_ppir::item_data::{self, MethodOwner};
use text_size::TextSize;

use crate::{
    resolve::{Location, SymbolTarget, symbol_at, target_definition},
    syntax::find_token_at_offset,
    usages::usages_of,
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
    /// A class field and an interface field are coupled by SPELLING alone —
    /// the class satisfies the interface with no `field as class_field`
    /// link. Keeping them in step would mean writing that link, which is an
    /// insertion, not a replacement of an existing name.
    ImplicitInterfaceField {
        /// The interface whose field is satisfied by spelling.
        interface: String,
        /// The name both declarations share.
        field: String,
    },
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
            Self::ImplicitInterfaceField { interface, field } => write!(
                f,
                "`{field}` satisfies field `{field}` of interface `{interface}` by \
                 spelling alone; write `{field} as {field}` in the `implements \
                 {interface}` block first, then rename"
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

    // The declarations were collected and vetted by `renameable`; each
    // group member's references join them here. `usages_at` includes an
    // ITEM's declaration but not a member's or a local's, so the union is
    // what makes the set complete, and the dedup is what keeps a span two
    // members both report from being edited twice.
    let mut spans: Vec<Location> = target.declarations;
    for symbol in target.symbols {
        for reference in usages_of(db, file, symbol) {
            if !spans.contains(&reference) {
                spans.push(reference);
            }
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
struct Renameable<'db> {
    /// The token under the cursor: what the editor highlights.
    cursor: Location,
    /// The name every edited span must currently spell.
    name: String,
    /// Every declaration the edit rewrites, deduplicated and all confirmed
    /// to live in the workspace.
    declarations: Vec<Location>,
    /// The group members whose references join the edit.
    symbols: Vec<SymbolTarget<'db>>,
}

/// The one gate both entry points pass, so `prepareRename` and `rename`
/// can never disagree about what is renameable.
fn renameable(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
) -> Result<Renameable<'_>, RenameError> {
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

    // The cursor must address something with a declaration before the group
    // is worth building.
    let cursor_declaration = target_definition(db, target).ok_or(RenameError::NotASymbol)?;
    let group = rename_group(db, file, target)?;
    debug_assert!(
        group.symbols.contains(&target),
        "a rename group always contains the symbol it was built for; \
         without it the edit renames everything EXCEPT what the cursor points at"
    );

    // EVERY declaration, not just the cursor's. A workspace `implements`
    // block can fill a stdlib interface's slot, and the slot's declaration
    // is then read-only source shared by every other implementor —
    // checking only the name under the cursor would have rewritten the
    // standard library.
    let mut declarations: Vec<Location> = Vec::new();
    for declaration in group
        .symbols
        .iter()
        .filter_map(|&symbol| target_definition(db, symbol))
        .chain(group.extra_declarations)
    {
        if declaration.file.source_root(db).kind(db) != baml_base::SourceRootKind::Workspace {
            return Err(RenameError::NotInWorkspace { name });
        }
        if !declarations.contains(&declaration) {
            declarations.push(declaration);
        }
    }
    debug_assert!(
        declarations.contains(&cursor_declaration),
        "the cursor's own declaration is always in the edit"
    );

    Ok(Renameable {
        cursor,
        name,
        declarations,
        symbols: group.symbols,
    })
}

// ── the rename group ─────────────────────────────────────────────────────────

/// Everything one rename must edit, before references are searched.
///
/// Most of it is symbols — things with a declaration of their own AND
/// references to find. The rest is declaration spans that are not symbols:
/// an `implements` block's `field as class_field` link names the
/// interface's field and the class's field, but declares neither.
struct RenameGroup<'db> {
    symbols: Vec<SymbolTarget<'db>>,
    extra_declarations: Vec<Location>,
}

impl<'db> RenameGroup<'db> {
    /// A symbol that answers for itself.
    fn solo(target: SymbolTarget<'db>) -> Self {
        RenameGroup {
            symbols: vec![target],
            extra_declarations: Vec::new(),
        }
    }
}

/// Every declaration the language forces to share `target`'s spelling, or
/// why a correct rename cannot be produced.
///
/// An interface member's name is not a local choice. The compiler pairs an
/// `implements` block's members with the interface's BY NAME and rejects
/// every mismatch — a method the impl omits is `MissingInterfaceMethod`,
/// one it adds is `UnknownInterfaceMember`, an unsatisfied field is
/// `MissingInterfaceField` — so the interface's declaration and every
/// impl's must move together or the program stops compiling. That makes
/// this group a compiler rule rather than a heuristic, which is the only
/// basis on which a rename may edit a file the cursor is not in.
///
/// The refusals are measured gaps, not caution:
///
/// - An enum VARIANT is found in expression position (`Status.Active`) but
///   not in a match *type pattern* (`Status.Active =>`): inference records
///   pattern types and no per-name pattern resolutions. Closing it is a
///   compiler-side pattern-resolution record.
/// - An ASSOCIATED TYPE's declarations are all reachable here (the
///   interface's `type Item` and each impl's `type Item = …`), but its
///   references live in TYPE positions — `Self.Item`, `T.Item` — which
///   carry no per-node resolution record at all, the same record-less
///   class as match type-patterns. The declarations alone are not a
///   rename.
fn rename_group<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    anchor: SourceFile,
    target: SymbolTarget<'db>,
) -> Result<RenameGroup<'db>, RenameError> {
    match target {
        SymbolTarget::Item(_) | SymbolTarget::Local { .. } => Ok(RenameGroup::solo(target)),
        SymbolTarget::Variant { .. } => Err(RenameError::IncompleteReferences {
            kind: "an enum variant",
        }),
        SymbolTarget::AssociatedType { .. } => Err(RenameError::IncompleteReferences {
            kind: "an associated type",
        }),
        // A method's group is decided by its OWNER, not its kind: a
        // class-inherent method answers for itself (a class method can
        // never satisfy an interface — the compiler reports
        // `MissingInterfaceMethod` even when the names match), while an
        // interface's and an impl's are two views of one slot.
        SymbolTarget::Method { func } => match item_data::method_owner(db, func) {
            None | Some(MethodOwner::Class(_)) => Ok(RenameGroup::solo(target)),
            Some(MethodOwner::Interface(iface)) => {
                let name = &item_data::function_data(db, func).name;
                Ok(interface_method_group(db, anchor, iface, name))
            }
            Some(MethodOwner::Impl(block)) => {
                // Normalize to the slot the impl is filling. A block whose
                // header does not resolve names no interface, so there is
                // nothing to keep in step with and nothing to search from.
                let iface =
                    block_interface(db, block).ok_or(RenameError::IncompleteReferences {
                        kind: "a method of an `implements` block whose interface does not resolve",
                    })?;
                let name = &item_data::function_data(db, func).name;
                Ok(interface_method_group(db, anchor, iface, name))
            }
        },
        SymbolTarget::Field { class, field_index } => class_field_group(db, class, field_index),
        SymbolTarget::InterfaceField { iface, field_index } => {
            interface_field_group(db, anchor, iface, field_index)
        }
    }
}

/// One interface method slot: the interface's own declaration plus every
/// `implements` block's override of it.
fn interface_method_group<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    anchor: SourceFile,
    iface: InterfaceLoc<'db>,
    name: &Name,
) -> RenameGroup<'db> {
    // The interface's own declaration. `methods` holds required and default
    // alike as real items, so this is one lookup rather than two views.
    let mut symbols: Vec<SymbolTarget<'db>> = item_data::interface_data(db, iface)
        .methods
        .iter()
        .filter(|&&func| item_data::function_data(db, func).name == *name)
        .map(|&func| SymbolTarget::Method { func })
        .collect();
    for block in impls_of(db, anchor, iface) {
        symbols.extend(
            item_data::impl_block_data(db, block)
                .methods
                .iter()
                .filter(|&&func| item_data::function_data(db, func).name == *name)
                .map(|&func| SymbolTarget::Method { func }),
        );
    }
    RenameGroup {
        symbols,
        extra_declarations: Vec::new(),
    }
}

/// One interface field: its declaration plus the interface-field side of
/// every `field as class_field` link that satisfies it.
///
/// A class may also satisfy a field by declaring one of the same name with
/// no link at all, which couples the two spellings with no token to
/// rewrite — see [`RenameError::ImplicitInterfaceField`].
fn interface_field_group<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    anchor: SourceFile,
    iface: InterfaceLoc<'db>,
    field_index: usize,
) -> Result<RenameGroup<'db>, RenameError> {
    let iface_data = item_data::interface_data(db, iface);
    let name = iface_data
        .fields
        .get(field_index)
        .map(|field| field.name.clone())
        .ok_or(RenameError::NotASymbol)?;

    let mut extra_declarations = Vec::new();
    for block in impls_of(db, anchor, iface) {
        let data = item_data::impl_block_data(db, block);
        let source_map = item_data::impl_block_source_map(db, block);
        let mut linked = false;
        for (index, link) in data.field_links.iter().enumerate() {
            if link.interface_field != name {
                continue;
            }
            linked = true;
            if let Some(spans) = source_map.field_links.get(index) {
                extra_declarations.push(Location {
                    file: block.file(db),
                    range: spans.interface_field_span,
                });
            }
        }
        if linked {
            continue;
        }
        // No link, so the obligation is being met by a class field of the
        // same name — unless the class has none, in which case the program
        // already fails `MissingInterfaceField` and nothing is coupled to
        // this spelling.
        let satisfied_by_spelling =
            item_data::impl_enclosing_class(db, block).is_some_and(|class| {
                item_data::class_data(db, class)
                    .fields
                    .iter()
                    .any(|field| field.name == name)
            });
        if satisfied_by_spelling {
            return Err(RenameError::ImplicitInterfaceField {
                interface: iface_data.name.to_string(),
                field: name.to_string(),
            });
        }
    }
    Ok(RenameGroup {
        symbols: vec![SymbolTarget::InterfaceField { iface, field_index }],
        extra_declarations,
    })
}

/// One class field: its declaration plus the class-field side of every
/// `field as class_field` link that names it.
///
/// A field-bearing interface can only be implemented in the class body
/// (`OutOfBodyImplementsFieldInterface`), so the `implements` blocks in the
/// class's own file are the complete set of links that can name this field
/// — no search of other files can add one.
fn class_field_group<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    class: ClassLoc<'db>,
    field_index: usize,
) -> Result<RenameGroup<'db>, RenameError> {
    let name = item_data::class_data(db, class)
        .fields
        .get(field_index)
        .map(|field| field.name.clone())
        .ok_or(RenameError::NotASymbol)?;

    let mut extra_declarations = Vec::new();
    for block in class_impls(db, class) {
        let data = item_data::impl_block_data(db, block);
        let source_map = item_data::impl_block_source_map(db, block);
        let mut linked = false;
        for (index, link) in data.field_links.iter().enumerate() {
            if link.class_field != name {
                continue;
            }
            linked = true;
            if let Some(spans) = source_map.field_links.get(index) {
                extra_declarations.push(Location {
                    file: block.file(db),
                    range: spans.class_field_span,
                });
            }
        }
        if linked {
            continue;
        }
        // No link names this field — but the interface may still be leaning
        // on it, because a class field satisfies an interface field of the
        // same name with nothing written down (`MissingInterfaceField`
        // fires the moment the spellings diverge).
        let Some(iface) = block_interface(db, block) else {
            continue;
        };
        let iface_data = item_data::interface_data(db, iface);
        if iface_data.fields.iter().any(|field| field.name == name) {
            return Err(RenameError::ImplicitInterfaceField {
                interface: iface_data.name.to_string(),
                field: name.to_string(),
            });
        }
    }
    Ok(RenameGroup {
        symbols: vec![SymbolTarget::Field { class, field_index }],
        extra_declarations,
    })
}

/// The `implements` blocks belonging to `class` — in-body, and the
/// out-of-body ones the lowering attaches to it.
fn class_impls<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    class: ClassLoc<'db>,
) -> Vec<ImplLoc<'db>> {
    item_data::file_impls(db, class.file(db))
        .iter()
        .copied()
        .filter(|&block| item_data::impl_enclosing_class(db, block) == Some(class))
        .collect()
}

/// The interface an `implements` block implements, or `None` when its
/// header does not resolve to one.
fn block_interface<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    block: ImplLoc<'db>,
) -> Option<InterfaceLoc<'db>> {
    baml_compiler2_hir_ty::interfaces::impl_data(db, block)
        .as_ref()
        .ok()
        .map(|data| data.interface)
}

/// Every `implements` block naming `iface`, from every workspace
/// viewpoint.
///
/// An impl in one workspace root of an interface declared in another is
/// visible from the first and not the second, so a single viewpoint would
/// miss it — this is the scope [`crate::usages_at`] searches, expressed as
/// viewpoints.
fn impls_of<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    anchor: SourceFile,
    iface: InterfaceLoc<'db>,
) -> Vec<ImplLoc<'db>> {
    let mut viewers: Vec<baml_base::SourceRoot> =
        baml_compiler2_hir::package::workspace_roots(db).clone();
    let anchor_root = anchor.source_root(db);
    if !viewers.contains(&anchor_root) {
        viewers.push(anchor_root);
    }

    let mut blocks = Vec::new();
    for viewer in viewers {
        for &block in baml_compiler2_hir_ty::impls::impls_naming_interface(db, viewer, iface) {
            if !blocks.contains(&block) {
                blocks.push(block);
            }
        }
    }
    blocks
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
    use crate::test_support::{CursorTest, TestDbExt};

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
        let test = at("\n    Active", "Active");
        assert_eq!(
            rename(&test.db, test.cursor.file, test.cursor.offset, "other"),
            Err(RenameError::IncompleteReferences {
                kind: "an enum variant"
            })
        );
        // `prepareRename` refuses too, so the editor greys F2 out rather
        // than failing after the reader has typed a new name.
        assert!(prepare_rename(&test.db, test.cursor.file, test.cursor.offset).is_err());

        // An associated type's DECLARATIONS are all reachable, but its
        // references live in type positions that record no resolution.
        let assoc = CursorTest::new(
            r#"interface Holds {
    type <[CURSOR]Item
    function get(self) -> Self.Item throws never
}
"#,
        );
        assert_eq!(
            rename(&assoc.db, assoc.cursor.file, assoc.cursor.offset, "Elem"),
            Err(RenameError::IncompleteReferences {
                kind: "an associated type"
            })
        );
    }

    /// An interface method and every `implements` block's override of it are
    /// one name as far as the compiler is concerned: omit the override and
    /// the impl fails `MissingInterfaceMethod`, rename only the override and
    /// it fails `UnknownInterfaceMember`. So all three entry points — the
    /// interface's declaration, an impl's, and a call — produce the SAME
    /// edit, and it covers both implementations.
    #[test]
    fn an_interface_method_renames_with_every_implementation() {
        const SRC: &str = r#"interface Shows {
    function show(self) -> string throws never
}

class B { v: int }

implement Shows for B {
    function show(self) -> string throws never { "b" }
}

class C { v: int }

implement Shows for C {
    function show(self) -> string throws never { "c" }
}

function call(b: B, s: Shows) -> string throws never { b.show() + s.show() }
"#;
        let edits = |marked: String| -> Vec<String> {
            let test = CursorTest::new(&marked);
            let mut spans: Vec<String> =
                rename(&test.db, test.cursor.file, test.cursor.offset, "render")
                    .unwrap_or_else(|error| unreachable!("an interface method renames: {error}"))
                    .iter()
                    .map(|span| test.format_file_range(span.file, span.range))
                    .collect();
            spans.sort();
            spans
        };

        let from_interface =
            edits(SRC.replacen("    function show", "    function <[CURSOR]show", 1));
        assert_eq!(
            from_interface,
            vec![
                // Sorted as text: impl C's override, both calls, the
                // interface's declaration, impl B's override.
                "test.baml:14:14",
                "test.baml:17:58",
                "test.baml:17:69",
                "test.baml:2:14",
                "test.baml:8:14",
            ],
            "got {from_interface:?}"
        );

        let from_impl = edits(SRC.replace(
            "for B {\n    function show",
            "for B {\n    function <[CURSOR]show",
        ));
        assert_eq!(from_impl, from_interface, "an impl names the same slot");

        let from_call = edits(SRC.replace("b.show()", "b.<[CURSOR]show()"));
        assert_eq!(from_call, from_interface, "a call names the same slot");
    }

    /// A class field that an `implements` block links to an interface field
    /// carries a second declaration — the class-field side of
    /// `iface_field as class_field`. Missing it left the link naming a field
    /// that no longer existed (`UnknownClassFieldInInterfaceLink`), which is
    /// exactly the half-applied rename this module exists to prevent.
    #[test]
    fn a_class_field_rename_covers_the_interface_link_that_names_it() {
        let test = CursorTest::new(
            r#"interface Named { name: string }

class P {
    <[CURSOR]label: string

    implements Named {
        name as label
    }
}

function read(p: P) -> string throws never { p.label }
"#,
        );
        let mut edits: Vec<String> =
            rename(&test.db, test.cursor.file, test.cursor.offset, "caption")
                .expect("a class field is renameable")
                .iter()
                .map(|span| test.format_file_range(span.file, span.range))
                .collect();
        edits.sort();
        assert_eq!(
            edits,
            vec![
                // The declaration, the `as` side of the link, and the read.
                "test.baml:11:48",
                "test.baml:4:5",
                "test.baml:7:17",
            ],
            "got {edits:?}"
        );
    }

    /// The interface side of the same link moves with the interface's own
    /// field declaration, and with every read through the interface.
    #[test]
    fn an_interface_field_rename_covers_its_links_and_reads() {
        let test = CursorTest::new(
            r#"interface Named {
    <[CURSOR]name: string
    function greet(self) -> string throws never { self.name }
}

class P {
    label: string

    implements Named {
        name as label
    }
}

function read(n: Named) -> string throws never { n.name }
"#,
        );
        let mut edits: Vec<String> =
            rename(&test.db, test.cursor.file, test.cursor.offset, "title")
                .expect("an interface field with explicit links is renameable")
                .iter()
                .map(|span| test.format_file_range(span.file, span.range))
                .collect();
        edits.sort();
        assert_eq!(
            edits,
            vec![
                // The declaration, `self.name`, the link's interface side,
                // and `n.name`.
                "test.baml:10:9",
                "test.baml:14:52",
                "test.baml:2:5",
                "test.baml:3:56",
            ],
            "got {edits:?}"
        );
    }

    /// A class field with no link satisfies a same-named interface field by
    /// SPELLING — nothing is written down, so there is no token to carry the
    /// new name and no correct edit to make. Both ends refuse, and the
    /// message says what to write first.
    #[test]
    fn a_field_coupled_by_spelling_alone_is_refused() {
        const SRC: &str = r#"interface Named {
    name: string
}

class P {
    name: string

    implements Named {}
}
"#;
        for marked in [
            // The class's field...
            SRC.replace(
                "\n    name: string\n\n    implements",
                "\n    <[CURSOR]name: string\n\n    implements",
            ),
            // ...and the interface's.
            SRC.replacen("    name: string", "    <[CURSOR]name: string", 1),
        ] {
            let test = CursorTest::new(&marked);
            let error = rename(&test.db, test.cursor.file, test.cursor.offset, "title")
                .expect_err("spelling-only coupling has no token to rewrite");
            assert_eq!(
                error,
                RenameError::ImplicitInterfaceField {
                    interface: "Named".to_string(),
                    field: "name".to_string(),
                }
            );
            assert!(
                error.to_string().contains("name as name"),
                "the message names the link to write: {error}"
            );
        }
    }

    /// The property the whole feature claims, checked by EXECUTION rather
    /// than by trusting a span set: apply the edits and the program still
    /// compiles. Every position in this fixture that F2 accepts is renamed
    /// and re-checked, so a group that misses a coupled declaration fails
    /// here with the compiler's own diagnostic.
    #[test]
    fn applying_a_rename_leaves_the_program_compiling() {
        const SRC: &str = r#"interface Named {
    name: string
    function greet(self) -> string throws never { self.name }
}

interface Shows {
    function show(self) -> string throws never
}

class P {
    label: string

    implements Named {
        name as label
    }

    implements Shows {
        function show(self) -> string throws never { self.label }
    }
}

class Q { v: int }

implement Shows for Q {
    function show(self) -> string throws never { "q" }
}

function read(p: P, n: Named, s: Shows) -> string throws never {
    p.label + n.name + s.show() + p.greet()
}
"#;
        // Each is a distinct member of a rename group: an interface field,
        // the class field its link names, an interface method with two
        // implementations, and one of those implementations.
        for (marker, old) in [
            ("    name: string", "name"),
            ("    label: string", "label"),
            ("    function show(self) -> string throws never\n", "show"),
            ("implement Shows for Q {\n    function show", "show"),
            ("    function greet", "greet"),
        ] {
            let start = SRC
                .find(marker)
                .unwrap_or_else(|| unreachable!("fixture contains {marker:?}"));
            let at = SRC[start..]
                .find(old)
                .unwrap_or_else(|| unreachable!("{marker:?} is followed by {old:?}"))
                + start;
            let test = CursorTest::new(&format!("{}<[CURSOR]{}", &SRC[..at], &SRC[at..]));

            let new_name = format!("{old}_renamed");
            let mut edits = rename(&test.db, test.cursor.file, test.cursor.offset, &new_name)
                .unwrap_or_else(|error| unreachable!("`{old}` at {marker:?} renames: {error}"));
            // Apply back to front so earlier offsets stay valid.
            edits.sort_by_key(|span| std::cmp::Reverse(span.range.start()));
            let mut edited = SRC.to_string();
            for span in &edits {
                edited.replace_range(std::ops::Range::<usize>::from(span.range), &new_name);
            }

            let mut db = baml_db::ProjectDatabase::default();
            let root = std::path::Path::new("/rename-check");
            db.workspace(root);
            db.file(&root.join("test.baml"), &edited);
            let errors: Vec<String> = baml_db::testing::check_user_files(&db)
                .iter()
                .filter(|diagnostic| {
                    diagnostic.severity == baml_db::baml_compiler_diagnostics::Severity::Error
                })
                .map(|diagnostic| format!("{:?}: {}", diagnostic.id, diagnostic.message))
                .collect();
            assert!(
                errors.is_empty(),
                "renaming `{old}` to `{new_name}` broke the program: {errors:?}\n{edited}"
            );
        }
    }

    /// A workspace `implements` block can fill a STDLIB interface's slot,
    /// and the slot's declaration is then read-only source. Refusing the
    /// whole edit is the only answer: rewriting the override alone leaves
    /// `UnknownInterfaceMember`, and rewriting the interface is not ours to
    /// do. Checking only the cursor's own declaration would have missed
    /// this, because the override under the cursor IS in the workspace.
    #[test]
    fn a_group_reaching_read_only_source_is_refused() {
        let test = CursorTest::new(
            r#"class Money {
    cents: int

    implements baml.ToString {
        function <[CURSOR]to_string(self) -> string throws never { "money" }
    }
}
"#,
        );
        let error = rename(&test.db, test.cursor.file, test.cursor.offset, "render")
            .expect_err("the interface's declaration is stdlib source");
        assert!(
            matches!(error, RenameError::NotInWorkspace { .. }),
            "got {error:?}"
        );
        assert!(prepare_rename(&test.db, test.cursor.file, test.cursor.offset).is_err());
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
