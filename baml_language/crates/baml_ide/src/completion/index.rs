//! Index slots: `t[<here>]`.
//!
//! A tuple receiver's index selects a slot, so the slots themselves are
//! offered as the constant indices that read them, each detailed with the
//! slot's type. A union of tuples reads only the positions every variant
//! has — whatever their total arities — typed by the join of the variants'
//! slot types. A list or map index is an ordinary expression; the
//! expression provider offers those names.

use super::completions::Completions;

pub(crate) fn complete(
    db: &dyn baml_compiler2_hir::Db,
    receiver: &baml_type::Ty,
    optional: bool,
    out: &mut Completions<'_>,
) {
    let receiver = if optional {
        receiver.strip_null()
    } else {
        receiver.clone()
    };
    let Some(variants) = crate::resolve::tuple_variants(db, &receiver) else {
        return;
    };
    for (slot, ty) in crate::resolve::common_tuple_slots(&variants)
        .iter()
        .enumerate()
    {
        out.add_tuple_slot(slot, ty);
    }
}
