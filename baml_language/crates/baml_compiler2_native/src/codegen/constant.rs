//! Constants.

use baml_compiler2_mir::{Constant, function_display_name};

use super::{Expr, Gen, Generator, unsupported};
use crate::{analysis::Repr, render::float_literal};

impl<'db> Generator<'_, 'db> {
    pub(super) fn constant(&self, constant: &Constant<'db>) -> Gen<Expr> {
        Ok(match constant {
            Constant::Int(n) => Expr::new(format!("{n}i64"), Repr::Int),
            Constant::Float(f) => Expr::new(float_literal(*f), Repr::Float),
            Constant::Bool(b) => Expr::new(format!("{b}"), Repr::Bool),
            Constant::Null => Expr::new("Value::NULL", Repr::Value),
            Constant::OmittedArg => Expr::new("Value::OMITTED_ARG", Repr::Value),
            // The literal's compile-time string object, as bytecode loads it:
            // no allocation per evaluation.
            Constant::String(s) => match self.index.strings.get(s.as_str()) {
                Some(object) => Expr::new(
                    format!("Value::object(vm.idx_to_ptr(ObjectIndex::from_raw({object})))"),
                    Repr::Value,
                ),
                None => Expr::new(format!("vm.string_value({s:?})"), Repr::Value),
            },
            Constant::Bigint(_) => return unsupported("bigint literal"),
            // A named function as a value: the program's pooled wrapper over
            // it, as bytecode loads it, so `f === f` holds as there.
            Constant::Function(func) => {
                let Some((object, _)) = self.index.function(self.db, *func)? else {
                    let name = function_display_name(self.db, *func);
                    return unsupported(format!("function `{name}` not found in the program"));
                };
                let object = self
                    .index
                    .pooled_generic_function(object, &[])
                    .unwrap_or(object);
                Expr::new(
                    format!("Value::object(vm.idx_to_ptr(ObjectIndex::from_raw({object})))"),
                    Repr::Value,
                )
            }
            // `foo<int>`: the program's pooled, interned instantiation.
            Constant::GenericFunction { func, type_args } => {
                let name = function_display_name(self.db, *func);
                let Some((object, _)) = self.index.function(self.db, *func)? else {
                    return unsupported(format!("function `{name}` not found in the program"));
                };
                let Some(object) = self.index.pooled_generic_function(object, type_args) else {
                    return unsupported(format!(
                        "generic function `{name}` used as a value with type arguments the program does not pool"
                    ));
                };
                Expr::new(
                    format!("Value::object(vm.idx_to_ptr(ObjectIndex::from_raw({object})))"),
                    Repr::Value,
                )
            }
            Constant::GlobalItem(_) => return unsupported("top-level let"),
            Constant::EnumVariant { .. } => return unsupported("enum variant literal"),
        })
    }
}
