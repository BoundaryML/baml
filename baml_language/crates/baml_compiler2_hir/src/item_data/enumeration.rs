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

use crate::{
    ids::{ClassMarker, LocalItemId},
    loc::{ClassLoc, EnumLoc, FunctionLoc, ImplLoc, InterfaceLoc, LetLoc, TypeAliasLoc},
};

/// Declare an enumeration query: `file_<plural>(file) -> Vec<XLoc>`, ordered by
/// source position. Synthesized companions share their parent's span, so they
/// sort beside it.
macro_rules! file_items {
    ($(#[$meta:meta])* $name:ident, $map:ident, $loc:ident) => {
        $(#[$meta])*
        #[salsa::tracked(returns(ref))]
        pub fn $name(db: &dyn crate::Db, file: SourceFile) -> Vec<$loc<'_>> {
            let item_tree = crate::file_item_tree(db, file);
            let mut items: Vec<_> = item_tree.$map.iter().collect();
            // Source position, then a stable tiebreaker for items that share an
            // offset: companions sit at their parent's offset, and
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
    /// Every class in `file`, in source order.
    file_classes,
    classes,
    ClassLoc
);
file_items!(
    /// Every function in `file`, in source order (companions beside their parent). Includes methods.
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
    /// Every type alias in `file`, in source order.
    file_type_aliases,
    type_aliases,
    TypeAliasLoc
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
    item_tree
        .impl_order
        .iter()
        .map(|id| ImplLoc::new(db, file, *id))
        .collect()
}

/// Whether `block` implements for `class` itself: written in the class's
/// body (its recorded owner — identity, never the name, so two classes of
/// one name keep their own blocks), or out-of-body for exactly that class
/// (the AST's rule, `impl_is_for_class`). Every other block has a for-type
/// of its own and is found through it.
fn impl_is_for_class(
    block: &crate::item_tree::ImplBlock,
    class_id: LocalItemId<ClassMarker>,
    class: &crate::item_tree::Class,
) -> bool {
    match block.origin {
        baml_compiler2_ast::ImplOrigin::InBody => block.owner == Some(class_id),
        baml_compiler2_ast::ImplOrigin::OutOfBody => baml_compiler2_ast::ast::impl_is_for_class(
            block.origin,
            block.generics.is_empty(),
            &block.for_target,
            &class.name,
        ),
    }
}

/// The `implements` blocks that are for `class` itself (see
/// `impl_is_for_class`), in source order.
#[salsa::tracked(returns(ref))]
pub fn class_impls<'db>(db: &'db dyn crate::Db, class: ClassLoc<'db>) -> Vec<ImplLoc<'db>> {
    let file = class.file(db);
    let item_tree = crate::file_item_tree(db, file);
    let class_data = &item_tree[class.id(db)];
    item_tree
        .impl_order
        .iter()
        .filter(|id| impl_is_for_class(&item_tree.impls[id], class.id(db), class_data))
        .map(|id| ImplLoc::new(db, file, *id))
        .collect()
}

/// The class `block` is for, when it is for a class of its own file (see
/// `impl_is_for_class`); `None` for a free block.
pub fn impl_class<'db>(db: &'db dyn crate::Db, block: ImplLoc<'db>) -> Option<ClassLoc<'db>> {
    let file = block.file(db);
    let item_tree = crate::file_item_tree(db, file);
    let data = &item_tree.impls[&block.id(db)];
    super::file_classes(db, file)
        .iter()
        .copied()
        .find(|class| impl_is_for_class(data, class.id(db), &item_tree[class.id(db)]))
}
