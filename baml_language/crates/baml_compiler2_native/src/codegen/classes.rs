//! What the type table says of a class beyond what the collector needs:
//! where its description is (see [`crate::json`]), and which classes are
//! the ones the runtime makes instances of itself.
//!
//! A class of `baml.panics` is a panic: its instances carry
//! [`layout::HEADER_PANIC`] in their headers, which is what a handler that
//! takes every error tests to let one pass.

use baml_compiler2_hir::package::lang_roots;
use baml_compiler2_hir_ty::{
    extern_loc::ClassRef,
    layout::{class_generic_params, class_head, lang_class, package_classes},
};

use super::{Backend, Translator, internal, table_word};
use crate::{
    CompileError, layout,
    narrow::TestKind,
    types::{NativeTy, TypeArg, class_layout},
};

/// The classes the runtime knows by position.
pub(super) struct WellKnown {
    /// The type id of each class of [`layout::WELL_KNOWN`], in its order.
    ids: Vec<usize>,
    /// The type id of each class of [`layout::NAMED`], in its order: zero
    /// until the program calls a native whose declaration names it.
    named: [usize; layout::NAMED.len()],
}

/// The types that pass a test, as a tag says them.
#[derive(Default)]
pub(super) struct Passing {
    pub(super) ids: Vec<usize>,
    /// Every panic passes: decided by the flag of the instance's header.
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

    /// Describe the type `type_id`, just added for `class` at `arguments`
    /// with this layout, and have the type say where its description is.
    pub(super) fn describe_class(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        type_id: usize,
        class: ClassRef<'db>,
        arguments: &[TypeArg<'db>],
        fields: &[NativeTy<'db>],
    ) {
        let at = self.json.class_type(db, class, arguments, type_id, fields);
        let described = layout::TYPE_DESCRIBED | at;
        self.types[type_id + 12..type_id + 16].copy_from_slice(&described.to_ne_bytes());
        self.identify_described(db);
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
            self.well_known = Some(WellKnown {
                ids,
                named: [0; layout::NAMED.len()],
            });
        }
        self.well_known
            .as_ref()
            .ok_or_else(|| internal("the well-known classes were just found"))
    }

    /// Give every class the declaration of `native` names a type and a
    /// description, for the native to be passed an instance or to build
    /// one, and note the type id of each the runtime finds by position.
    /// A class one of them holds in a field is described with it, and the
    /// native finds it there: one that builds a `Timeout` builds the
    /// `Duration` its field says.
    pub(super) fn name_classes(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        native: &crate::natives::Native,
    ) -> Result<(), CompileError> {
        for path in native.classes {
            let class = crate::natives::class_at(db, *path)
                .ok_or_else(|| internal(format!("the standard library declares `{}`", path.1)))?;
            let fields = class_layout(db, class, &[]).map_err(internal)?;
            let id = self.class_type(db, class, &[], &fields);
            let listed = layout::NAMED.iter().position(|(namespace, name)| {
                lang_class(db, baml_base::LangPackage::Baml, &[namespace], name) == Some(class)
            });
            if let Some(position) = listed {
                self.well_known(db)?;
                if let Some(known) = &mut self.well_known {
                    known.named[position] = id;
                }
            }
        }
        Ok(())
    }

    /// Finish the type table's types: after the last, the list of
    /// well-known types and then of named ones, which the table's first
    /// word locates.
    pub(super) fn finish_types(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
    ) -> Result<(), CompileError> {
        let known = self.well_known(db)?;
        let listed: Vec<usize> = known.ids.iter().chain(&known.named).copied().collect();
        let list = table_word(self.types.len());
        self.types[..4].copy_from_slice(&list.to_ne_bytes());
        for word in
            std::iter::once(table_word(listed.len())).chain(listed.into_iter().map(table_word))
        {
            self.types.extend_from_slice(&word.to_ne_bytes());
        }
        self.align_types();
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

    /// The type id of `class`, which takes no type arguments.
    pub(super) fn plain_class(&mut self, class: ClassRef<'db>) -> Result<usize, CompileError> {
        if !class_generic_params(self.db, class).is_empty() {
            return Err(internal(
                "a checked class tag is of a class without type parameters",
            ));
        }
        let fields = class_layout(self.db, class, &[]).map_err(internal)?;
        Ok(self.backend.class_type(self.db, class, &[], &fields))
    }

    /// The type id of every panic class.
    pub(super) fn panic_ids(&mut self) -> Result<Vec<usize>, CompileError> {
        let known = self.backend.well_known(self.db)?;
        Ok(known.ids[..layout::WELL_KNOWN_PANICS].to_vec())
    }

    /// Whether `id` is the type id of a panic class.
    pub(super) fn is_panic_id(&mut self, id: usize) -> Result<bool, CompileError> {
        let known = self.backend.well_known(self.db)?;
        Ok(known.ids[..layout::WELL_KNOWN_PANICS].contains(&id))
    }
}

/// Stands in a list of passing type ids for "any panic".
pub(super) const ANY_PANIC: usize = usize::MAX;
