//! Field slots in an object literal: `Foo { <here> }`.
//!
//! The literal's class is the one INFERENCE recorded for it, so a
//! constructor still being typed resolves exactly as the compiler resolved
//! it, and fields already written are offered once.

use super::completions::Completions;
use crate::resolve::ObjectLiteralPosition;

pub(crate) fn complete(
    db: &dyn baml_compiler2_hir::Db,
    literal: &ObjectLiteralPosition<'_>,
    out: &mut Completions<'_>,
) {
    let data = baml_compiler2_hir::item_data::class_data(db, literal.class);
    let types = baml_compiler2_hir_ty::lower::resolve_class_fields(db, literal.class);
    for (index, field) in data.fields.iter().enumerate() {
        if literal.written.contains(&field.name) {
            continue;
        }
        out.add_record_field(literal.class, field, types.get(index).map(|(_, ty, _)| ty));
    }
}
