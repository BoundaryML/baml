//! Calls of the standard library's natives: Rust functions of the runtime,
//! reached by symbol with the C calling convention.

use baml_compiler2_mir::Operand;
use cranelift_codegen::ir::{self, AbiParam, InstBuilder, types};
use cranelift_module::{Linkage, Module};

use super::{TRAP_AFTER_PANIC, Translator, WORD, internal};
use crate::{
    CompileError,
    natives::{Kind, Native, Ty},
    types::NativeTy,
};

/// Called with the thrown object when a native throws and nothing catches
/// it. Does not return.
const THROW_UNCAUGHT: &str = "baml_rt_throw_uncaught";

fn abi_param(ty: &Ty) -> Option<AbiParam> {
    match ty {
        Ty::Int | Ty::String | Ty::Array(_) => Some(AbiParam::new(WORD)),
        Ty::Float => Some(AbiParam::new(types::F64)),
        // The C ABI has the caller extend an argument narrower than a
        // register.
        Ty::Bool => Some(AbiParam::new(types::I8).uext()),
        Ty::Null => None,
    }
}

impl<'db> Translator<'_, 'db> {
    fn import(
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
    /// has. A native that throws ends the program: nothing catches yet.
    pub(super) fn native_call(
        &mut self,
        native: &'static Native,
        arguments: &[Operand<'db>],
    ) -> Result<(ir::Value, NativeTy<'db>), CompileError> {
        let (signature, kind) = native.callable().map_err(internal)?;
        let takes_worker = kind != Kind::Leaf;

        let mut params = Vec::new();
        let mut values = Vec::new();
        if takes_worker {
            params.push(AbiParam::new(WORD));
            values.push(self.mutator());
        }
        for (argument, ty) in arguments.iter().zip(signature.params) {
            params.push(abi_param(ty).ok_or_else(|| internal("a native takes no `null`"))?);
            values.push(self.operand_as(argument, &ty.native())?);
        }
        let returns = if native.throws {
            vec![AbiParam::new(types::I64), AbiParam::new(WORD)]
        } else {
            match &signature.result {
                Ty::Bool => vec![AbiParam::new(types::I8)],
                ty => abi_param(ty).into_iter().collect(),
            }
        };
        let callee = self.import(native.symbol, params, returns)?;

        if takes_worker {
            // The native can collect, which moves storage.
            self.forget_arrays();
        }
        let call = self.builder.ins().call(callee, &values);
        self.read_worker();
        let results = self.builder.inst_results(call).to_vec();

        let result = if native.throws {
            let (bits, thrown) = (results[0], results[1]);
            let raise = self.builder.create_block();
            let resume = self.builder.create_block();
            self.builder.set_cold_block(raise);
            self.builder.ins().brif(thrown, raise, &[], resume, &[]);
            self.builder.seal_block(raise);
            self.builder.seal_block(resume);

            self.builder.switch_to_block(raise);
            let uncaught = self.import(THROW_UNCAUGHT, vec![AbiParam::new(WORD)], Vec::new())?;
            self.builder.ins().call(uncaught, &[thrown]);
            self.builder.ins().trap(TRAP_AFTER_PANIC);

            self.builder.switch_to_block(resume);
            match &signature.result {
                Ty::Float => self
                    .builder
                    .ins()
                    .bitcast(types::F64, ir::MemFlags::new(), bits),
                Ty::Bool => self.builder.ins().ireduce(types::I8, bits),
                Ty::Null => self.builder.ins().iconst(types::I8, 0),
                Ty::Int | Ty::String | Ty::Array(_) => bits,
            }
        } else {
            match results.first() {
                Some(value) => *value,
                None => self.builder.ins().iconst(types::I8, 0),
            }
        };
        Ok((result, signature.result.native()))
    }
}
