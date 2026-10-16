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
            // A named function as a value: its object, as bytecode loads it.
            Constant::Function(func) => {
                let name = function_display_name(self.db, *func);
                let Some(object) = self.index.objects.get(&name) else {
                    return unsupported(format!("function `{name}` not found in the program"));
                };
                Expr::new(
                    format!("Value::object(vm.idx_to_ptr(ObjectIndex::from_raw({object})))"),
                    Repr::Value,
                )
            }
            Constant::GenericFunction { .. } => {
                return unsupported("generic function used as a value");
            }
            Constant::GlobalItem(_) => return unsupported("top-level let"),
            Constant::EnumVariant { .. } => return unsupported("enum variant literal"),
        })
    }
}
