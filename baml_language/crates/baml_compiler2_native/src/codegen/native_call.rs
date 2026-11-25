//! Calls of the standard library's natives: Rust functions of the runtime,
//! reached by symbol with the C calling convention.
//!
//! A native takes a value of a type that fixes its tag as one word, with no
//! tag, and a value of any other type as two, a tag and a payload. One
//! that cannot throw returns its result the same way. One that can returns
//! two words whatever its result is: its result as a pair, or what it
//! threw with [`layout::THROWN`] set in the tag.

use baml_compiler2_mir::Operand;
use cranelift_codegen::ir::{self, AbiParam, InstBuilder, types};
use cranelift_module::{Linkage, Module};

use super::{Translator, WORD, internal, of_pair_words, of_payload, of_slot_words, slot_words};
use crate::{
    CompileError,
    check::native_type_arguments,
    layout,
    natives::{Native, Ty},
    types::NativeTy,
};

/// The sort of an array of `int`, `float`, `string` or `bigint`, whose one
/// type argument is the type of the array.
const RUST_SORT: &str = "baml._rust_sort";

/// What a native takes for a parameter of type `ty`, without the id a
/// [`Ty::Typed`] is preceded by: one word, or the two of a value.
pub(super) fn abi_params(ty: &Ty) -> Result<Vec<AbiParam>, CompileError> {
    Ok(match ty {
        _ if ty.is_value() => vec![AbiParam::new(WORD); 2],
        Ty::Float => vec![AbiParam::new(types::F64)],
        // The C ABI has the caller extend an argument narrower than a
        // register.
        Ty::Bool => vec![AbiParam::new(types::I8).uext()],
        Ty::Null => return Err(internal("a native takes no `null`")),
        _ => vec![AbiParam::new(WORD)],
    })
}

impl<'db> Translator<'_, 'db> {
    pub(super) fn import(
        &mut self,
        symbol: &str,
        params: Vec<AbiParam>,
        returns: Vec<AbiParam>,
    ) -> Result<ir::FuncRef, CompileError> {
        let mut signature = ir::Signature::new(self.backend.call_conv());
        signature.params = params;
        signature.returns = returns;
        // Declaring a symbol again with the same signature names the same
        // function.
        let id = self
            .backend
            .module
            .declare_function(symbol, Linkage::Import, &signature)
            .map_err(internal)?;
        Ok(self.function_ref(id))
    }

    /// What a native takes for `value`, the argument of a parameter
    /// declared `ty`, which the call holds as a value of type `expected`.
    fn native_argument(
        &mut self,
        ty: &Ty,
        expected: &NativeTy<'db>,
        value: ir::Value,
    ) -> Result<Vec<ir::Value>, CompileError> {
        let (tag, payload) = match ty {
            // As a slot of the type holds it: an array or a map the native
            // puts it in is of that type.
            Ty::Elem(_) | Ty::Optional(_) => slot_words(&mut self.builder, expected, value),
            _ if ty.is_value() => self.pair_words(expected, value)?,
            // The address of Rust state, which is held as any value is.
            _ if expected.is_pair() => return Ok(vec![self.builder.ins().isplit(value).1]),
            _ => return Ok(vec![value]),
        };
        Ok(vec![tag, payload])
    }

    /// Call `native` with `arguments` and return its result and the type it
    /// has, or do without the call what the native would.
    pub(super) fn native_call(
        &mut self,
        native: &'static Native,
        arguments: &[Operand<'db>],
        ntypeargs: usize,
    ) -> Result<(ir::Value, NativeTy<'db>), CompileError> {
        if native.path == super::string_slice::STRING_SLICE {
            return self.inline_string_slice(native, arguments, ntypeargs);
        }
        if let Some(result) = self.inline_map_native(native, arguments, ntypeargs)? {
            return Ok(result);
        }
        self.native_call_out_of_line(native, arguments, ntypeargs)
    }

    /// Call `native` with `arguments` and return its result and the type it
    /// has. What a native throws goes to the handler of the calling block
    /// when it has one, and unwinds from here otherwise.
    pub(super) fn native_call_out_of_line(
        &mut self,
        native: &'static Native,
        arguments: &[Operand<'db>],
        ntypeargs: usize,
    ) -> Result<(ir::Value, NativeTy<'db>), CompileError> {
        let (signature, flags) = native.callable().map_err(internal)?;
        let takes_worker = flags.allocates;
        let types = native_type_arguments(self.db, self.body, native, arguments, ntypeargs)
            .map_err(internal)?;
        let (type_arguments, arguments) = arguments.split_at(ntypeargs);
        self.backend.name_classes(self.db, native)?;

        let mut params = Vec::new();
        let mut values = Vec::new();
        if takes_worker {
            params.push(AbiParam::new(WORD));
            values.push(self.mutator());
        }
        // The type id of each type argument, for a native that is passed
        // them: after the worker and before the values.
        let described = super::json::takes_descriptions(native);
        if native.type_ids != 0 {
            for (argument, ty) in type_arguments.iter().zip(&types) {
                params.push(AbiParam::new(types::I32));
                values.push(match ty {
                    _ if described => self.described_type(argument)?,
                    // An array has no type id of its own, and its words do
                    // not say what order they are sorted in: the sort is
                    // told what the elements are.
                    NativeTy::Array(element) if native.path == RUST_SORT => {
                        let id = self.backend.type_id(self.db, element)?.unwrap_or(0);
                        self.builder.ins().iconst(types::I32, super::imm(id))
                    }
                    _ => self.type_id(argument)?,
                });
            }
        }
        for (argument, ty) in arguments.iter().zip(signature.params) {
            let expected = ty.native(self.db, &types);
            if matches!(ty, Ty::Typed) {
                // What the value is, as far as its static type says.
                let found = self.operand_ty(argument)?;
                if found.holds_any() && described {
                    // Only its tag will say what the value is.
                    self.describe_program_types();
                }
                let id = if described {
                    self.backend.describe_native(self.db, &found)?
                } else {
                    // What compares two values: it reads the description
                    // it is given, and the value's tag for the rest.
                    if let Some(class) = self.program.equals_unlisted.first() {
                        return Err(internal(format!(
                            "comparing values in a program with a generic class that has an eq \
                             of its own, which the runtime cannot call: {class}"
                        )));
                    }
                    self.backend.compares_values = true;
                    self.backend.describe_native_if_any(self.db, &found)
                };
                params.push(AbiParam::new(types::I32));
                values.push(self.builder.ins().iconst(types::I32, super::imm(id)));
            }
            let value = self.operand_as(argument, &expected)?;
            params.extend(abi_params(ty)?);
            values.extend(self.native_argument(ty, &expected, value)?);
        }
        let returns = if native.throws || signature.result.is_value() {
            vec![AbiParam::new(WORD); 2]
        } else {
            match &signature.result {
                Ty::Bool => vec![AbiParam::new(types::I8)],
                Ty::Float => vec![AbiParam::new(types::F64)],
                Ty::Null => Vec::new(),
                _ => vec![AbiParam::new(WORD)],
            }
        };
        let produced = signature.result.native(self.db, &types);
        if takes_worker {
            // The native can collect, which moves storage.
            self.forget_arrays();
        }
        // The two words of what a native returned as a pair, or the one of
        // any other.
        let results = if flags.is_async {
            // It runs compiled code, which throws by unwinding, so it is
            // called as a BAML function is, through one.
            let stub = self.backend.async_stub(native, &params)?;
            let stub = self.function_ref(stub);
            let returned = self.baml_call(&super::unwind::Called::Direct(stub), &values[1..])?;
            let (tag, payload) = self.builder.ins().isplit(returned);
            vec![tag, payload]
        } else {
            let callee = self.import(native.symbol, params, returns)?;
            let call = self.builder.ins().call(callee, &values);
            let results = self.builder.inst_results(call).to_vec();
            if native.throws {
                let payload = self.returned_or_raise(results[0], results[1])?;
                vec![results[0], payload]
            } else {
                results
            }
        };
        let result = match (&signature.result, results.as_slice()) {
            // The value of a type parameter as a slot of the type holds
            // it: an array or a map the native took it from is of that
            // type.
            (Ty::Elem(_), [tag, payload]) => {
                of_slot_words(&mut self.builder, &produced, *tag, *payload)
            }
            // `null`, or the value of a type parameter as a slot of the
            // type holds it, which for an enum variant is not how a pair
            // holds one.
            (Ty::Optional(Ty::Elem(index)), [tag, payload])
                if matches!(types[*index], NativeTy::Enum(_)) =>
            {
                self.optional_variant(&types[*index], *tag, *payload)?
            }
            // A pair, or what one known to hold a value of the result's
            // type holds.
            (ty, [tag, payload]) if ty.is_value() => {
                of_pair_words(&mut self.builder, &produced, *tag, *payload)
            }
            // The result of a native that can throw, which is not a pair:
            // its payload.
            (_, [_, payload] | [payload]) if produced.is_pair() => {
                // Rust state: a reference held as any value is.
                self.pair_of_pointer(*payload)
            }
            (Ty::Float | Ty::Bool, [value]) => *value,
            (_, [_, payload] | [payload]) => of_payload(&mut self.builder, *payload, &produced),
            _ => self.builder.ins().iconst(types::I8, 0),
        };
        Ok((result, produced))
    }

    /// The pair an optional enum variant is, from the two words a slot of
    /// the enum's type holds a variant as, or those of `null`.
    fn optional_variant(
        &mut self,
        variant: &NativeTy<'db>,
        tag: ir::Value,
        position: ir::Value,
    ) -> Result<ir::Value, CompileError> {
        let (held, boxed) = self.pair_words(variant, position)?;
        let null = self.word_const(layout::TAG_NULL);
        let held = self.builder.ins().select(tag, held, null);
        let boxed = self.builder.ins().select(tag, boxed, null);
        Ok(self.pair(held, boxed))
    }
}
