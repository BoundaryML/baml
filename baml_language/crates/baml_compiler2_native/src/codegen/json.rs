//! What compiled code passes `baml.json` and anything else that takes a
//! type: a type object, which says where the type's description is. See
//! [`crate::json`].

use baml_compiler2_mir::Operand;
use cranelift_codegen::ir::{self, InstBuilder};

use super::{Backend, Translator, WORD, classes::ClassLike, internal};
use crate::{CompileError, json::Described, natives::Native, types::NativeTy};

pub(super) fn takes_descriptions(native: &Native) -> bool {
    crate::json::takes_descriptions(native)
}

impl<'db> Backend<'db> {
    /// Have every description of a class or an enum made since the last
    /// call say where its class object is. Making a class object describes
    /// its class, which can describe more.
    pub(super) fn identify_described(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
    ) -> Result<(), CompileError> {
        loop {
            let pending = self.json.take_pending();
            if pending.is_empty() {
                return Ok(());
            }
            for pending in pending {
                let what = match pending.what {
                    Described::Class(class) => ClassLike::Class(class),
                    Described::Enum(enum_ref) => ClassLike::Enum(enum_ref),
                };
                let class = self.class_object(db, what)?;
                self.json.identify(pending.at, class);
            }
        }
    }
}

impl<'db> Translator<'_, 'db> {
    /// The type object of the type a local of type `type` holds, as a
    /// function or a native that takes its type arguments is passed it:
    /// what this function was itself passed for one of its own type
    /// parameters, and otherwise the object of the type the program names.
    pub(super) fn type_argument(
        &mut self,
        loaded: &Operand<'db>,
    ) -> Result<ir::Value, CompileError> {
        let template = self
            .body
            .type_value(loaded)
            .ok_or_else(|| internal("a checked type argument is a type loaded once"))?;
        if let baml_compiler2_mir::TyTemplate::TypeArgRef(slot) = template
            && let Some(passed) = self.type_args.get(&(*slot as usize))
        {
            return Ok(self.builder.use_var(*passed));
        }
        let object = if template.is_fully_concrete() {
            self.backend
                .type_object_spelled(self.db, &template.substitute_symbolic(&[]))?
                .0
        } else {
            // A type that names a type parameter this function is not
            // passed the type of: each parameter is any type.
            match crate::types::template_ty(self.db, template, self.body.arguments) {
                Some(ty) => self.backend.type_object_of(self.db, &ty)?.0,
                None => self.backend.type_object_of_nothing()?,
            }
        };
        Ok(self.data_address(object))
    }

    /// What a native is passed for the static type `ty` of a value: its
    /// type object, or zero when only the value's tag says what it is,
    /// which is so of a type with no description unless `described`: a
    /// native that reads descriptions is checked to be passed one.
    pub(super) fn static_type(
        &mut self,
        ty: &NativeTy<'db>,
        described: bool,
    ) -> Result<ir::Value, CompileError> {
        if *ty != NativeTy::Unknown {
            let (object, at) = self.backend.type_object_of(self.db, ty)?;
            if at != 0 {
                return Ok(self.data_address(object));
            }
            if described {
                return Err(internal("a checked json value has a described type"));
            }
        }
        Ok(self.builder.ins().iconst(WORD, 0))
    }

    /// Describe every class and enum the program makes values of: what a
    /// value written by its tag alone can be. One the runtime cannot
    /// write is left out, and writing a value of it is refused when the
    /// program runs.
    pub(super) fn describe_program_types(&mut self) -> Result<(), CompileError> {
        let program = self.program;
        for ty in &program.types {
            let described = match ty {
                NativeTy::Class(_, arguments) => arguments.is_empty(),
                NativeTy::Enum(_) => true,
                _ => false,
            };
            if described {
                self.backend.json.describe_native_if_any(self.db, ty);
            }
        }
        self.backend.identify_described(self.db)
    }
}
