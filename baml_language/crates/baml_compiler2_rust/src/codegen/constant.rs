//! Constants.

use baml_compiler2_mir::{Constant, enum_display_name, function_display_name};

use super::{Expr, Gen, Generator, unsupported};
use crate::{admit::Rejection, analysis::Repr, render::float_literal};

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
            // Its compile-time bigint object, as bytecode loads it.
            Constant::Bigint(value) => match self.index.bigints.get(&value.to_string()) {
                Some(object) => Expr::new(
                    format!("Value::object(vm.idx_to_ptr(ObjectIndex::from_raw({object})))"),
                    Repr::Value,
                ),
                None => {
                    return Err(Rejection::Invalid(
                        "bigint literal not in the program's pool".into(),
                    ));
                }
            },
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
            // A top-level `let`: the value `$init` left in its global slot,
            // read as this function reads globals.
            Constant::GlobalItem(binding) => {
                let Some(slot) = self.index.let_slot(self.db, *binding)? else {
                    return unsupported("top-level let not found in the program");
                };
                Expr::new(
                    format!(
                        "vm.load_global(vm.idx_to_ptr(ObjectIndex::from_raw({})), {slot})",
                        self.object
                    ),
                    Repr::Value,
                )
            }
            // A fresh variant object, as bytecode's `AllocVariant` allocates;
            // its range check is done here, against the enum's declaration.
            Constant::EnumVariant { enum_ref, index } => {
                let Some(object) = self.index.enum_(self.db, *enum_ref)? else {
                    let name = enum_display_name(self.db, *enum_ref);
                    return unsupported(format!("enum `{name}` not found in the program"));
                };
                let variants = self.index.enum_variants(object);
                if *index as usize >= variants {
                    return Err(Rejection::Invalid(format!(
                        "variant #{index} of an enum with {variants} variants"
                    )));
                }
                Expr::new(
                    format!(
                        "vm.new_variant(vm.idx_to_ptr(ObjectIndex::from_raw({object})), {index})"
                    ),
                    Repr::Value,
                )
            }
        })
    }
}
