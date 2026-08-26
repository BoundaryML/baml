//! Field slots in an object literal: `Foo { <here> }`.
//!
//! The literal's class is the one INFERENCE recorded for it, so a
//! constructor still being typed resolves exactly as the compiler resolved
//! it, and fields already written are offered once.

use baml_base::SourceFile;

use super::completions::Completions;
use crate::resolve::ObjectLiteralPosition;

pub(crate) fn complete(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    literal: &ObjectLiteralPosition<'_>,
    out: &mut Completions,
) {
    let data = baml_compiler2_ppir::item_data::class_data(db, literal.class);
    let types = baml_compiler2_hir_ty::lower::resolve_class_fields(db, literal.class);
    for (index, field) in data.fields.iter().enumerate() {
        if literal.written.contains(&field.name) {
            continue;
        }
        out.add_record_field(db, file, field, types.get(index).map(|(_, ty, _)| ty));
    }
}
