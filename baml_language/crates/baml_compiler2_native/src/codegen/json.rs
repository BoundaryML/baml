//! What compiled code passes `baml.json`: the description of a type, made
//! the first time a call names the type. See [`crate::json`].

use baml_compiler2_mir::{Operand, TyTemplate};
use cranelift_codegen::ir::{self, InstBuilder, types};
use cranelift_module::{DataDescription, FuncId, Module};

use super::{Backend, Translator, imm, internal};
use crate::{
    CompileError,
    json::Described,
    json_layout::{DESCRIBED, JSON_SECTION},
    natives::Native,
    types::NativeTy,
};

pub(super) fn takes_descriptions(native: &Native) -> bool {
    crate::json::takes_descriptions(native)
}

impl<'db> Backend<'db> {
    /// Write the type id of every class and enum described since the last
    /// call into its description.
    pub(super) fn identify_described(&mut self, db: &'db dyn baml_compiler2_mir::Db) {
        for pending in self.json.take_pending() {
            let id = match pending.what {
                Described::Class(class) => {
                    self.class_type(db, class, &pending.arguments, &pending.layout)
                }
                Described::Enum(enum_ref) => self.enum_type(db, enum_ref),
            };
            self.json.identify(pending.at, id, pending.listed);
        }
    }

    /// What a `baml.json` native is passed for the type `ty`.
    pub(super) fn describe(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        ty: &baml_compiler2_mir::RuntimeTy,
    ) -> Result<usize, CompileError> {
        let at = self.json.describe(db, ty).map_err(internal)?;
        self.identify_described(db);
        Ok((DESCRIBED | at) as usize)
    }

    /// What a `baml.json` native is passed for the static type of a value:
    /// zero when only the value's tag says what it is.
    pub(super) fn describe_native(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        ty: &NativeTy<'db>,
    ) -> Result<usize, CompileError> {
        let at = self.json.describe_native(db, ty).map_err(internal)?;
        self.identify_described(db);
        Ok(if at == 0 {
            0
        } else {
            (DESCRIBED | at) as usize
        })
    }

    /// [`Self::describe_native`], or zero for a type with no description.
    pub(super) fn describe_native_if_any(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        ty: &NativeTy<'db>,
    ) -> usize {
        let at = self.json.describe_native_if_any(db, ty);
        self.identify_described(db);
        if at == 0 {
            0
        } else {
            (DESCRIBED | at) as usize
        }
    }

    /// End the type table with the descriptions, and say where they start.
    /// Returns what the table needs of the linker: the address of each
    /// entry a description names.
    pub(super) fn append_descriptions(&mut self) -> DataDescription {
        let json = std::mem::replace(&mut self.json, crate::json::JsonTypes::new());
        self.align_types();
        let start = super::table_word(self.types.len());
        self.types[JSON_SECTION..JSON_SECTION + 4].copy_from_slice(&start.to_ne_bytes());
        let mut description = DataDescription::new();
        for (at, entry) in json.addresses() {
            let entry = self
                .module
                .declare_func_in_data(FuncId::from_u32(*entry), &mut description);
            description.write_function_addr(start + at, entry);
        }
        self.types.extend(json.section());
        self.align_types();
        description
    }
}

impl<'db> Translator<'_, 'db> {
    /// The description of the type a local of type `type` holds, as a
    /// `baml.json` native is passed it: what this function was itself
    /// passed for one of its own type parameters, and otherwise the
    /// description of the type the program names.
    pub(super) fn described_type(
        &mut self,
        loaded: &Operand<'db>,
    ) -> Result<ir::Value, CompileError> {
        let template = self
            .body
            .type_value(loaded)
            .ok_or_else(|| internal("a checked type argument is a type loaded once"))?;
        if let TyTemplate::TypeArgRef(slot) = template
            && let Some(passed) = self.type_ids.get(*slot as usize)
        {
            return Ok(self.builder.use_var(*passed));
        }
        // A copy of a function made for one type: that type.
        if let TyTemplate::TypeArgRef(slot) = template {
            let known = self.body.arguments.get(*slot as usize).cloned().flatten();
            let known = known.ok_or_else(|| internal("a type argument with no type"))?;
            let id = self.backend.describe_native(self.db, &known)?;
            return Ok(self.builder.ins().iconst(types::I32, imm(id)));
        }
        if !template.is_fully_concrete() {
            return Err(internal(
                "a checked json type argument names no type parameter",
            ));
        }
        let id = self
            .backend
            .describe(self.db, &template.substitute_symbolic(&[]))?;
        Ok(self.builder.ins().iconst(types::I32, imm(id)))
    }

    /// Describe the type a call passes a function for one of its type
    /// parameters, in case the function hands it on to `baml.json`: a type
    /// with a type id is passed as that, and the runtime finds its
    /// description by it.
    pub(super) fn describe_type_argument(&mut self, loaded: &Operand<'db>) {
        if let Some(template) = self.body.type_value(loaded)
            && template.is_fully_concrete()
            && self
                .backend
                .json
                .describe(self.db, &template.substitute_symbolic(&[]))
                .is_ok()
        {
            // A type id not yet in the table is added by this.
            self.backend.identify_described(self.db);
        }
    }

    /// Describe every class and enum the program makes values of: what a
    /// value written by its tag alone can be. One the runtime cannot
    /// write is left out, and writing a value of it is refused when the
    /// program runs.
    pub(super) fn describe_program_types(&mut self) {
        let program = self.program;
        for ty in &program.types {
            let described = match ty {
                NativeTy::Class(_, arguments) => arguments.is_empty(),
                NativeTy::Enum(_) => true,
                _ => false,
            };
            if described {
                let _ = self.backend.json.describe_native(self.db, ty);
            }
        }
        self.backend.identify_described(self.db);
    }
}
