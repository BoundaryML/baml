//! Calls of the standard library's natives: Rust functions of the runtime,
//! reached by symbol with the C calling convention.

use baml_compiler2_mir::Operand;
use cranelift_codegen::ir::{self, AbiParam, InstBuilder, types};
use cranelift_module::{Linkage, Module};

use super::{Translator, WORD, internal};
use crate::{
    CompileError,
    check::native_type_arguments,
    natives::{Native, Ty},
    types::NativeTy,
};

/// The sort of an array of `int`, `float`, `string` or `bigint`, whose one
/// type argument is the type of the array.
const RUST_SORT: &str = "baml._rust_sort";

fn abi_param(ty: &Ty) -> Option<AbiParam> {
    match ty {
        // An optional scalar is two words, which its user passes one by
        // one; any other optional is a reference.
        Ty::Int
        | Ty::String
        | Ty::Bigint
        | Ty::Bytes
        | Ty::Array(_)
        | Ty::Map
        | Ty::Optional(_)
        | Ty::Elem(_)
        | Ty::Any
        | Ty::Typed
        | Ty::BoxedElem(_)
        | Ty::Class(_)
        | Ty::Rust => Some(AbiParam::new(WORD)),
        Ty::Float => Some(AbiParam::new(types::F64)),
        // The C ABI has the caller extend an argument narrower than a
        // register.
        Ty::Bool => Some(AbiParam::new(types::I8).uext()),
        Ty::Null => None,
    }
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
        // The type id of each type argument, as a generic function is
        // passed them: after the worker and before the values.
        let described = super::json::takes_descriptions(native);
        for (argument, ty) in type_arguments.iter().zip(&types) {
            params.push(AbiParam::new(types::I32));
            values.push(match ty {
                _ if described => self.described_type(argument)?,
                // An array has no type id of its own, and its words do not
                // say whether they are `int`s or `float`s: the sort is
                // told what the elements are.
                NativeTy::Array(element) if native.path == RUST_SORT => {
                    let id = self.backend.type_id(self.db, element)?.unwrap_or(0);
                    self.builder.ins().iconst(types::I32, super::imm(id))
                }
                _ => self.type_id(argument)?,
            });
        }
        if type_arguments.is_empty() {
            // A method of `Map`: the types are its receiver's.
            for ty in types.iter().take(native.type_args) {
                let id = self.backend.type_id(self.db, ty)?.unwrap_or(0);
                params.push(AbiParam::new(types::I32));
                values.push(self.builder.ins().iconst(types::I32, super::imm(id)));
            }
        }
        for (argument, ty) in arguments.iter().zip(signature.params) {
            let expected = ty.native(self.db, &types);
            if matches!(ty, Ty::Typed) {
                // What the value is, as far as its static type says.
                let found = self.operand_ty(argument)?;
                if found.is_boxed() {
                    // Only its header will say what the value is.
                    self.describe_program_types();
                }
                let id = self.backend.describe_native(self.db, &found)?;
                params.push(AbiParam::new(types::I32));
                values.push(self.builder.ins().iconst(types::I32, super::imm(id)));
            }
            let value = self.operand_as(argument, &expected)?;
            if matches!(ty, Ty::Elem(_)) {
                params.push(AbiParam::new(WORD));
                values.push(self.as_slot(value, &expected));
            } else if ty.is_pair() {
                // Its bits, then whether it is present.
                let (bits, present) = self.builder.ins().isplit(value);
                params.extend([AbiParam::new(types::I64); 2]);
                values.extend([bits, present]);
            } else {
                params.push(abi_param(ty).ok_or_else(|| internal("a native takes no `null`"))?);
                values.push(value);
            }
        }
        let returns = if native.throws {
            if signature.result.is_pair() || signature.result.is_optional_elem() {
                return Err(internal("a native that throws returns no optional scalar"));
            }
            vec![AbiParam::new(types::I64), AbiParam::new(WORD)]
        } else {
            match &signature.result {
                Ty::Bool => vec![AbiParam::new(types::I8)],
                // Two words in two registers: its bits, then whether it
                // is present.
                ty if ty.is_pair() || ty.is_optional_elem() => vec![AbiParam::new(types::I64); 2],
                ty => abi_param(ty).into_iter().collect(),
            }
        };
        let produced = signature.result.native(self.db, &types);
        if takes_worker {
            // The native can collect, which moves storage.
            self.forget_arrays();
        }
        // The bits of what a native that can throw returned, and the two
        // words of any other.
        let (thrown_or, results) = if flags.is_async {
            // It runs compiled code, which throws by unwinding, so it is
            // called as a BAML function is, through one.
            let stub = self.backend.async_stub(native, &params)?;
            let stub = self.function_ref(stub);
            let bits = self.baml_call(&super::unwind::Called::Direct(stub), &values[1..])?;
            (Some(bits), Vec::new())
        } else {
            let callee = self.import(native.symbol, params, returns)?;
            let call = self.builder.ins().call(callee, &values);
            let results = self.builder.inst_results(call).to_vec();
            if native.throws {
                let (bits, thrown) = (results[0], results[1]);
                let raise = self.builder.create_block();
                let resume = self.builder.create_block();
                self.builder.set_cold_block(raise);
                self.builder.ins().brif(thrown, raise, &[], resume, &[]);
                self.builder.seal_block(raise);
                self.builder.seal_block(resume);

                self.builder.switch_to_block(raise);
                self.raise(thrown, None)?;

                self.builder.switch_to_block(resume);
                (Some(bits), results)
            } else {
                (None, results)
            }
        };

        let result = if let Some(bits) = thrown_or {
            match &signature.result {
                Ty::Float => self
                    .builder
                    .ins()
                    .bitcast(types::F64, ir::MemFlags::new(), bits),
                Ty::Bool => self.builder.ins().ireduce(types::I8, bits),
                Ty::Null => self.builder.ins().iconst(types::I8, 0),
                Ty::Elem(_) => self.slot_as(bits, &produced),
                Ty::BoxedElem(_) => self.unboxed_result(bits, &produced),
                Ty::Int
                | Ty::String
                | Ty::Bigint
                | Ty::Bytes
                | Ty::Array(_)
                | Ty::Map
                | Ty::Optional(_)
                | Ty::Any
                | Ty::Typed
                | Ty::Class(_)
                | Ty::Rust => bits,
            }
        } else {
            match (&signature.result, results.as_slice()) {
                (Ty::Elem(_), [bits]) => self.slot_as(*bits, &produced),
                (Ty::BoxedElem(_), [bits]) => self.unboxed_result(*bits, &produced),
                // An absent reference is the zero its word already is.
                (_, [bits, _]) if produced.is_ref() => *bits,
                (_, [bits, present]) => self.builder.ins().iconcat(*bits, *present),
                (_, [value]) => *value,
                _ => self.builder.ins().iconst(types::I8, 0),
            }
        };
        Ok((result, produced))
    }
}
