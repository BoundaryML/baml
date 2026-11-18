//! What the type table says of a class beyond what the collector needs: its
//! name and its fields, for the runtime to write an instance, and which
//! classes are the ones the runtime makes instances of itself.
//!
//! A class of `baml.panics` is a panic: its instances carry
//! [`layout::HEADER_PANIC`] in their headers, which is what a handler that
//! takes every error tests to let one pass.

use baml_compiler2_hir::package::lang_roots;
use baml_compiler2_hir_ty::{
    extern_loc::ClassRef,
    layout::{class_fields, class_generic_params, class_head, lang_class, package_classes},
    render::Viewpoint,
};

use super::{Backend, Translator, internal, table_word};
use crate::{
    CompileError, layout,
    narrow::TestKind,
    types::{NativeTy, class_layout, field_slots},
};

/// A class's description, to be written after the last type.
pub(super) struct Description {
    /// The type it describes.
    type_id: usize,
    name: String,
    fields: Vec<DescribedField>,
}

struct DescribedField {
    name: String,
    offset: usize,
    kind: u32,
    enum_type: usize,
}

/// The classes the runtime knows by position.
pub(super) struct WellKnown {
    /// The type id of each class of [`layout::WELL_KNOWN`], in its order.
    ids: Vec<usize>,
}

/// The types that pass a test, as a header says them.
#[derive(Default)]
pub(super) struct Passing {
    pub(super) ids: Vec<usize>,
    /// Every panic passes: decided by the header's flag.
    pub(super) panics: bool,
    pub(super) null: bool,
}

/// Whether `class` is a panic: a class of `baml.panics`.
pub(crate) fn is_panic_class<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    class: ClassRef<'db>,
) -> bool {
    class_head(db, class).is_panic_type(lang_roots(db))
}

/// The name an uncaught throw of an instance of `class` is written with.
fn class_name<'db>(db: &'db dyn baml_compiler2_mir::Db, class: ClassRef<'db>) -> String {
    let head = class_head(db, class);
    let mut name = Viewpoint::canonical(db)
        .display_label(head.root())
        .to_string();
    for part in head.namespace() {
        name.push('.');
        name.push_str(part.as_str());
    }
    name.push('.');
    name.push_str(head.name().as_str());
    name
}

impl<'db> Backend<'db> {
    /// The header of a new instance of `class`, whose type id is `type_id`.
    pub(super) fn class_header(
        db: &'db dyn baml_compiler2_mir::Db,
        class: ClassRef<'db>,
        type_id: usize,
    ) -> usize {
        if is_panic_class(db, class) {
            type_id | layout::HEADER_PANIC
        } else {
            type_id
        }
    }

    /// Keep what the type `type_id`, just added for `class` with this
    /// layout, is described as.
    pub(super) fn describe_class(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        type_id: usize,
        class: ClassRef<'db>,
        fields: &[NativeTy<'db>],
    ) {
        let declared = class_fields(db, class);
        let fields = declared
            .iter()
            .zip(fields)
            .zip(field_slots(fields))
            .map(|(((name, _), ty), slot)| {
                let scalar = |ty: &NativeTy<'db>, backend: &mut Self| match ty {
                    NativeTy::Int => Some((layout::FIELD_INT, 0)),
                    NativeTy::Float => Some((layout::FIELD_FLOAT, 0)),
                    NativeTy::Bool => Some((layout::FIELD_BOOL, 0)),
                    NativeTy::Enum(enum_ref) => {
                        Some((layout::FIELD_ENUM, backend.enum_type(db, *enum_ref)))
                    }
                    _ => None,
                };
                let (kind, enum_type) = match ty {
                    NativeTy::Optional(inner) if !inner.is_ref() => match scalar(inner, self) {
                        Some((kind, enum_type)) => (kind | layout::FIELD_OPTIONAL, enum_type),
                        None => (layout::FIELD_OPAQUE, 0),
                    },
                    // `null` is stored as zero, which is how no object is
                    // written.
                    NativeTy::Null => (layout::FIELD_REF, 0),
                    ty if ty.is_ref() => (layout::FIELD_REF, 0),
                    ty => scalar(ty, self).unwrap_or((layout::FIELD_OPAQUE, 0)),
                };
                DescribedField {
                    name: name.to_string(),
                    offset: layout::field_offset(slot),
                    kind,
                    enum_type,
                }
            })
            .collect();
        self.descriptions.push(Description {
            type_id,
            name: class_name(db, class),
            fields,
        });
    }

    /// The type ids of the classes the runtime knows by position, adding
    /// their types the first time.
    pub(super) fn well_known(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
    ) -> Result<&WellKnown, CompileError> {
        if self.well_known.is_none() {
            let mut ids = Vec::with_capacity(layout::WELL_KNOWN.len());
            for (namespace, name) in layout::WELL_KNOWN {
                let class = lang_class(db, baml_base::LangPackage::Baml, &[namespace], name)
                    .ok_or_else(|| {
                        internal(format!(
                            "the standard library declares `baml.{namespace}.{name}`"
                        ))
                    })?;
                let fields = class_layout(db, class, &[]).map_err(internal)?;
                ids.push(self.class_type(db, class, &[], &fields));
            }
            // The header's flag stands for the whole of `baml.panics`, so
            // the runtime's list must be all of it.
            let root = lang_roots(db)
                .get(baml_base::LangPackage::Baml)
                .ok_or_else(|| internal("the standard library is installed"))?;
            let panics = package_classes(db, root)
                .filter(|class| is_panic_class(db, *class))
                .count();
            if panics != layout::WELL_KNOWN_PANICS {
                return Err(internal(
                    "the runtime's list of panics is the standard library's",
                ));
            }
            self.well_known = Some(WellKnown { ids });
        }
        self.well_known
            .as_ref()
            .ok_or_else(|| internal("the well-known classes were just found"))
    }

    /// Finish the type table: after the last type, the descriptions of the
    /// classes, and then the list of well-known types, which the table's
    /// first word locates.
    pub(super) fn finish_types(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
    ) -> Result<(), CompileError> {
        let well_known = self.well_known(db)?.ids.clone();
        let descriptions = std::mem::take(&mut self.descriptions);
        for description in descriptions {
            let at = self.types.len();
            let mut text = at
                + layout::DESCRIPTION_HEADER
                + layout::DESCRIPTION_FIELD * description.fields.len();
            let mut words = vec![
                table_word(text),
                table_word(description.name.len()),
                table_word(description.fields.len()),
            ];
            text += description.name.len();
            for field in &description.fields {
                words.extend([
                    table_word(text),
                    table_word(field.name.len()),
                    table_word(field.offset),
                    field.kind,
                    table_word(field.enum_type),
                ]);
                text += field.name.len();
            }
            for word in words {
                self.types.extend_from_slice(&word.to_ne_bytes());
            }
            self.types.extend_from_slice(description.name.as_bytes());
            for field in &description.fields {
                self.types.extend_from_slice(field.name.as_bytes());
            }
            self.types.resize(layout::words(self.types.len()), 0);
            let described = layout::TYPE_DESCRIBED | table_word(at);
            self.types[description.type_id + 12..description.type_id + 16]
                .copy_from_slice(&described.to_ne_bytes());
        }
        let list = table_word(self.types.len());
        self.types[..4].copy_from_slice(&list.to_ne_bytes());
        for word in std::iter::once(table_word(well_known.len()))
            .chain(well_known.into_iter().map(table_word))
        {
            self.types.extend_from_slice(&word.to_ne_bytes());
        }
        self.types.resize(layout::words(self.types.len()), 0);
        Ok(())
    }
}

impl<'db> Translator<'_, 'db> {
    /// The types that pass `test`, with every panic class taken out of the
    /// list when all of them pass: the header's flag says that.
    pub(super) fn passing(&mut self, test: &TestKind<'db>) -> Result<Passing, CompileError> {
        let mut ids = Vec::new();
        let null = self.passing_ids(test, &mut ids)?;
        let mut panics = ids.contains(&ANY_PANIC);
        ids.retain(|id| *id != ANY_PANIC);
        let known = self.backend.well_known(self.db)?;
        let panic_ids = &known.ids[..layout::WELL_KNOWN_PANICS];
        panics |= panic_ids.iter().all(|id| ids.contains(id));
        if panics {
            ids.retain(|id| !panic_ids.contains(id));
        }
        ids.sort_unstable();
        ids.dedup();
        Ok(Passing { ids, panics, null })
    }

    /// The type id of `class`, which takes no type arguments, and whether
    /// its instances are panics.
    pub(super) fn plain_class(
        &mut self,
        class: ClassRef<'db>,
    ) -> Result<(usize, bool), CompileError> {
        if !class_generic_params(self.db, class).is_empty() {
            return Err(internal(
                "a checked class tag is of a class without type parameters",
            ));
        }
        let fields = class_layout(self.db, class, &[]).map_err(internal)?;
        let id = self.backend.class_type(self.db, class, &[], &fields);
        Ok((id, is_panic_class(self.db, class)))
    }

    /// Whether `id` is the type id of a panic class.
    pub(super) fn is_panic_id(&mut self, id: usize) -> Result<bool, CompileError> {
        let known = self.backend.well_known(self.db)?;
        Ok(known.ids[..layout::WELL_KNOWN_PANICS].contains(&id))
    }
}

/// Stands in a list of passing type ids for "any panic".
pub(super) const ANY_PANIC: usize = usize::MAX;
