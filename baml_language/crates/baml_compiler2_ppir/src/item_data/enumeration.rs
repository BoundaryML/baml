//! Enumeration queries — "which items does this file declare?"
//!
//! The other half of the firewall. Consumers get a list of `*Loc`s and then look
//! each one up with a `*_data` query; they never touch the `ItemTree`'s maps, and
//! they never mint a `Loc` themselves.
//!
//! Each list is sorted by the item's source position, so iteration order is
//! deterministic (the `ItemTree`'s maps are `FxHashMap`s and are not). Sorting by
//! position is safe *here* — the result is a set of `Loc`s, which are
//! position-independent, so moving an item around in the file does not change
//! this query's value unless it actually reorders declarations.

use baml_base::SourceFile;
use baml_compiler2_hir::loc::{
    ClassLoc, EnumLoc, FunctionLoc, ImplLoc, InterfaceLoc, LetLoc, TemplateStringLoc, TypeAliasLoc,
};

/// Declare an enumeration query: `file_<plural>(file) -> Vec<XLoc>`, ordered by
/// source position. Synthetic `*$stream` companions carry no source span, so they
/// sort first (at offset 0); user-declared items follow in source order.
macro_rules! file_items {
    ($(#[$meta:meta])* $name:ident, $map:ident, $loc:ident) => {
        $(#[$meta])*
        #[salsa::tracked(returns(ref))]
        pub fn $name(db: &dyn crate::Db, file: SourceFile) -> Vec<$loc<'_>> {
            let item_tree = crate::file_item_tree(db, file);
            let mut items: Vec<_> = item_tree.$map.iter().collect();
            // Source position, then a stable tiebreaker for items that share an
            // offset: synthetic `*$stream` companions all sit at offset 0, and
            // `$map` is an `FxHashMap` (nondeterministic iteration), so ties must
            // be broken by a position-independent key — otherwise this query's
            // value (and its early-cutoff) would be unstable across rebuilds.
            items.sort_by(|(left_id, left), (right_id, right)| {
                left.span
                    .start()
                    .cmp(&right.span.start())
                    .then_with(|| left.name.cmp(&right.name))
                    .then_with(|| left_id.as_u32().cmp(&right_id.as_u32()))
            });
            items
                .into_iter()
                .map(|(id, _)| $loc::new(db, file, *id))
                .collect()
        }
    };
}

file_items!(
    /// Every class in `file`, in source order (synthetic `*$stream` companions first).
    file_classes,
    classes,
    ClassLoc
);
file_items!(
    /// Every function in `file`, in source order (synthetic companions first). Includes methods.
    file_functions,
    functions,
    FunctionLoc
);
file_items!(
    /// Every enum declared in `file`, in source order.
    file_enums,
    enums,
    EnumLoc
);
file_items!(
    /// Every interface declared in `file`, in source order.
    file_interfaces,
    interfaces,
    InterfaceLoc
);
file_items!(
    /// Every type alias in `file`, in source order (synthetic `*$stream` companions first).
    file_type_aliases,
    type_aliases,
    TypeAliasLoc
);
file_items!(
    /// Every template string declared in `file`, in source order.
    file_template_strings,
    template_strings,
    TemplateStringLoc
);
file_items!(
    /// Every top-level `let` declared in `file`, in source order.
    file_lets,
    lets,
    LetLoc
);

/// Every `implements` block in `file`, in source order — both in-body and
/// out-of-body.
///
/// Source order is load-bearing here: coherence needs a deterministic order to
/// pick a winner among overlapping impls.
#[salsa::tracked(returns(ref))]
pub fn file_impls(db: &dyn crate::Db, file: SourceFile) -> Vec<ImplLoc<'_>> {
    let item_tree = crate::file_item_tree(db, file);
    let mut items: Vec<_> = item_tree.impls.iter().collect();
    items.sort_by_key(|(_, block)| block.span.start());
    items
        .into_iter()
        .map(|(id, _)| ImplLoc::new(db, file, *id))
        .collect()
}

/// The out-of-body (`implement I for T`) blocks in `file`, in source order.
#[salsa::tracked(returns(ref))]
pub fn file_free_impls(db: &dyn crate::Db, file: SourceFile) -> Vec<ImplLoc<'_>> {
    let item_tree = crate::file_item_tree(db, file);
    item_tree
        .free_impls
        .iter()
        .map(|id| ImplLoc::new(db, file, *id))
        .collect()
}

/// The `implements` blocks whose subject is `class`, in source order.
#[salsa::tracked(returns(ref))]
pub fn class_impls<'db>(db: &'db dyn crate::Db, class: ClassLoc<'db>) -> Vec<ImplLoc<'db>> {
    let file = class.file(db);
    let item_tree = crate::file_item_tree(db, file);
    item_tree
        .class_to_impls
        .get(&class.id(db))
        .map(|impls| impls.iter().map(|id| ImplLoc::new(db, file, *id)).collect())
        .unwrap_or_default()
}
