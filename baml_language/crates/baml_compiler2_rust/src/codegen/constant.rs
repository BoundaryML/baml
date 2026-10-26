//! Generates constant expressions and resolves pooled objects.

use baml_compiler2_mir::{BlockId, Constant, enum_display_name, function_display_name};

use super::{Expr, Gen, Generator, unsupported};
use crate::{
    admit::Rejection,
    analysis::Repr,
    render::{float_literal, object_value},
};

impl<'db> Generator<'_, 'db> {
    pub(super) fn constant(&mut self, block: BlockId, constant: &Constant<'db>) -> Gen<Expr> {
        let code = match constant {
            Constant::Int(n) => return Ok(Expr::atom(format!("{n}i64"), Repr::Int)),
            Constant::Float(f) => return Ok(Expr::atom(float_literal(*f), Repr::Float)),
            Constant::Bool(b) => return Ok(Expr::atom(format!("{b}"), Repr::Bool)),
            Constant::Null => return Ok(Expr::atom("Value::NULL", Repr::Value)),
            Constant::OmittedArg => return Ok(Expr::atom("Value::OMITTED_ARG", Repr::Value)),
            Constant::String(s) => match self.index.strings.get(s.as_str()) {
                Some(&object) => object_value(object),
                None => format!("vm.string_value({s:?})"),
            },
            Constant::Bigint(value) => match self.index.bigints.get(&value.to_string()) {
                Some(&object) => object_value(object),
                None => {
                    return Err(Rejection::Invalid(
                        "bigint literal not in the program's pool".into(),
                    ));
                }
            },
            // Pool function wrappers so repeated reads preserve identity.
            Constant::Function(func) => {
                let Some((object, _)) = self.index.function(self.db, *func) else {
                    let name = function_display_name(self.db, *func);
                    return unsupported(format!("function `{name}` not found in the program"));
                };
                let object = self
                    .index
                    .pooled_generic_function(object, &[])
                    .unwrap_or(object);
                object_value(object)
            }
            Constant::GenericFunction { func, type_args } => {
                let name = function_display_name(self.db, *func);
                let Some((object, _)) = self.index.function(self.db, *func) else {
                    return unsupported(format!("function `{name}` not found in the program"));
                };
                let Some(object) = self.index.pooled_generic_function(object, type_args) else {
                    return unsupported(format!(
                        "generic function `{name}` used as a value with type arguments the program does not pool"
                    ));
                };
                object_value(object)
            }
            Constant::GlobalItem(binding) => {
                let Some(slot) = self.index.let_slot(self.db, *binding) else {
                    return unsupported("top-level let not found in the program");
                };
                return self.fallible_expr(
                    block,
                    &format!(
                        "vm.load_global_checked(vm.record_object(fr, ObjectIndex::from_raw({})), {slot})",
                        self.object
                    ),
                    Repr::Value,
                );
            }
            Constant::EnumVariant { enum_ref, index } => {
                let Some(object) = self.index.enum_(self.db, *enum_ref) else {
                    let name = enum_display_name(self.db, *enum_ref);
                    return unsupported(format!("enum `{name}` not found in the program"));
                };
                let variants = self.index.enum_variants(object);
                if *index as usize >= variants {
                    return Err(Rejection::Invalid(format!(
                        "variant #{index} of an enum with {variants} variants"
                    )));
                }
                format!(
                    "vm.new_variant(vm.record_object(fr, ObjectIndex::from_raw({object})), {index})"
                )
            }
        };
        Ok(Expr::new(code, Repr::Value))
    }
}
