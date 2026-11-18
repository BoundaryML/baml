//! Error unwinding: the table the runtime's unwinder finds handlers in, and
//! the code a throw, a rethrow and a call under a handler compile to.
//!
//! A frame with no handler pays nothing: its calls are plain calls and it
//! has no entry in the table. A call of a BAML function from a block that
//! unwinds to a handler is a `try_call`, whose exception edge the table
//! records by the call's return address. A throw whose handler is in the
//! same function is a jump to it. Any other throw calls the runtime, which
//! walks the frame records, finds the first return address the table has a
//! handler for, and resumes there with the error and its context in the two
//! payload registers.

use baml_compiler2_mir::{BlockId, Operand};
use cranelift_codegen::{
    CompiledCode, FinalizedMachExceptionHandler,
    ir::{
        self, BlockArg, ExceptionTableData, ExceptionTableItem, ExceptionTag, InstBuilder,
        SourceLoc,
    },
};
use cranelift_module::{DataDescription, FuncId, Linkage, Module};

use super::{Backend, TRAP_AFTER_PANIC, Translator, WORD, internal};
use crate::{CompileError, types::NativeTy};

/// The symbol of the table of call sites that have a handler.
pub(crate) const UNWIND_SYMBOL: &str = "baml_native_unwind";
/// The version of that table's format.
const UNWIND_VERSION: u32 = 1;

/// Set in a site's flags when its handler reads the context it lands with.
const SITE_WANTS_CONTEXT: u32 = 1;
/// Set in a site's flags when its handler takes every error. No handler
/// selects by tag yet, so every site has it.
const SITE_CATCHES_ALL: u32 = 2;

/// The exception tag of a handler that reads the context it lands with.
/// Tags select nothing: the unwinder stops at the first handler, and a
/// typed `catch` tests the error it landed with.
const TAG_WANTS_CONTEXT: u32 = 1;

/// Marks the instructions of a `defer` body, so that the ranges of code
/// they become can be read back from the compiled function.
const SHIELDED: u32 = 1;

/// A call with a handler.
pub(super) struct Site {
    /// The offset in the function of the call's return address.
    return_offset: u32,
    /// The offset in the function of the handler's first instruction.
    handler_offset: u32,
    /// The frame pointer minus the stack pointer at the call.
    frame_offset: u32,
    flags: u32,
}

/// What every function compiled so far contributes to the unwinder's
/// tables.
#[derive(Default)]
pub(super) struct Tables {
    sites: Vec<(FuncId, Vec<Site>)>,
    /// For each function, in the order of `Backend::functions`: the ranges
    /// of its code that run shielded from cancellation, and its name.
    pub(super) described: Vec<(Vec<(u32, u32)>, String)>,
}

impl Tables {
    /// Keep what the unwinder needs to know about the function just
    /// compiled.
    pub(super) fn record(
        &mut self,
        id: FuncId,
        name: &str,
        code: &CompiledCode,
    ) -> Result<(), CompileError> {
        let mut sites = Vec::new();
        for site in code.buffer.call_sites() {
            let handler = site
                .exception_handlers
                .iter()
                .find_map(|handler| match handler {
                    FinalizedMachExceptionHandler::Tag(tag, offset) => {
                        debug_assert_eq!(tag.as_u32(), TAG_WANTS_CONTEXT);
                        Some((*offset, SITE_WANTS_CONTEXT | SITE_CATCHES_ALL))
                    }
                    FinalizedMachExceptionHandler::Default(offset) => {
                        Some((*offset, SITE_CATCHES_ALL))
                    }
                    FinalizedMachExceptionHandler::Context(_) => None,
                });
            let Some((handler_offset, flags)) = handler else {
                continue;
            };
            sites.push(Site {
                return_offset: site.ret_addr,
                handler_offset,
                frame_offset: site
                    .frame_offset
                    .ok_or_else(|| internal("a call with a handler says where its frame is"))?,
                flags,
            });
        }
        if !sites.is_empty() {
            self.sites.push((id, sites));
        }

        let mut shields: Vec<(u32, u32)> = Vec::new();
        for range in code.buffer.get_srclocs_sorted() {
            if range.loc.is_default() || range.start == range.end {
                continue;
            }
            match shields.last_mut() {
                Some(last) if last.1 == range.start => last.1 = range.end,
                _ => shields.push((range.start, range.end)),
            }
        }
        self.described.push((shields, name.to_owned()));
        Ok(())
    }
}

impl Backend<'_> {
    /// Emit the table of calls that have a handler. Every field is
    /// little-endian and the table is aligned to 8 bytes:
    ///
    /// ```text
    /// +0   u32  format version (1)
    /// +4   u32  F, the number of functions that have such a call
    /// then for each function:
    ///        +0   u64  address of the function's first instruction
    ///        +8   u32  N, the number of its calls with a handler
    ///        +12  u32  zero
    ///        +16  N site records of 16 bytes:
    ///               +0   u32  offset in the function of the call's return
    ///                         address
    ///               +4   u32  offset in the function of the handler
    ///               +8   u32  the frame pointer minus the stack pointer
    ///                         at the call
    ///               +12  u32  flags: 1 the handler reads the context,
    ///                         2 the handler takes every error
    /// ```
    pub(super) fn define_unwind(&mut self) -> Result<(), CompileError> {
        let functions = std::mem::take(&mut self.unwind.sites);
        let mut description = DataDescription::new();
        let mut bytes: Vec<u8> = Vec::new();
        let count = u32::try_from(functions.len()).map_err(internal)?;
        for word in [UNWIND_VERSION, count] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        for (function, sites) in functions {
            let function = self.module.declare_func_in_data(function, &mut description);
            description
                .write_function_addr(u32::try_from(bytes.len()).map_err(internal)?, function);
            bytes.extend_from_slice(&0_u64.to_le_bytes());
            let count = u32::try_from(sites.len()).map_err(internal)?;
            for word in [count, 0] {
                bytes.extend_from_slice(&word.to_le_bytes());
            }
            for site in sites {
                for word in [
                    site.return_offset,
                    site.handler_offset,
                    site.frame_offset,
                    site.flags,
                ] {
                    bytes.extend_from_slice(&word.to_le_bytes());
                }
            }
        }
        let id = self
            .module
            .declare_data(UNWIND_SYMBOL, Linkage::Export, false, false)
            .map_err(internal)?;
        self.define_data(id, description, bytes)
    }
}

/// The BAML function a call names: one of this program by reference, or
/// code loaded from a closure or the method table, with its signature.
pub(super) enum Called {
    Direct(ir::FuncRef),
    Indirect(ir::SigRef, ir::Value),
}

/// A `try_call` of `called` whose exception edge goes to `landing`, and
/// the block the call returns to, which receives its result.
pub(super) fn try_call(
    builder: &mut cranelift_frontend::FunctionBuilder<'_>,
    called: &Called,
    arguments: &[ir::Value],
    landing: ir::Block,
    wants_context: bool,
) -> ir::Value {
    let signature = match called {
        Called::Direct(function) => builder.func.dfg.ext_funcs[*function].signature,
        Called::Indirect(signature, _) => *signature,
    };
    let result = builder.func.dfg.signatures[signature].returns[0].value_type;
    let returned = builder.create_block();
    builder.append_block_param(returned, result);
    let normal = builder
        .func
        .dfg
        .block_call(returned, &[BlockArg::TryCallRet(0)]);
    let thrown = builder
        .func
        .dfg
        .block_call(landing, &[BlockArg::TryCallExn(0), BlockArg::TryCallExn(1)]);
    let handler = if wants_context {
        ExceptionTableItem::Tag(ExceptionTag::from_u32(TAG_WANTS_CONTEXT), thrown)
    } else {
        ExceptionTableItem::Default(thrown)
    };
    let table = builder
        .func
        .dfg
        .exception_tables
        .push(ExceptionTableData::new(signature, normal, [handler]));
    match called {
        Called::Direct(function) => builder.ins().try_call(*function, arguments, table),
        Called::Indirect(_, code) => builder.ins().try_call_indirect(*code, arguments, table),
    };
    builder.seal_block(returned);
    builder.switch_to_block(returned);
    builder.block_params(returned)[0]
}

impl<'db> Translator<'_, 'db> {
    /// The handler a throw in the block being translated unwinds to, when
    /// it is in this function.
    fn pad(&self) -> Option<BlockId> {
        self.body.blocks[self.current].unwind
    }

    /// The block a throw to the handler `pad` jumps to with the error and
    /// its context: it puts them in the handler's locals and goes on to
    /// the handler.
    fn landing(&mut self, pad: BlockId) -> ir::Block {
        if let Some(landing) = self.landings.get(&pad.0) {
            return *landing;
        }
        let landing = self.builder.create_block();
        self.builder.append_block_param(landing, WORD);
        self.builder.append_block_param(landing, WORD);
        self.builder.set_cold_block(landing);
        self.landings.insert(pad.0, landing);
        landing
    }

    /// Whether the handler `pad` reads the context it lands with: only a
    /// rethrow does.
    fn wants_context(&self, pad: BlockId) -> bool {
        let Some(landing) = self.body.blocks[pad.0].landing else {
            return false;
        };
        self.body.blocks.iter().any(|block| {
            matches!(
                &block.terminator,
                Some(baml_compiler2_mir::Terminator::Rethrow {
                    context: Operand::Copy(place) | Operand::Move(place),
                    ..
                }) if *place == baml_compiler2_mir::Place::Local(landing.context_local)
            )
        })
    }

    /// Give every landing block its code. Called once the function's own
    /// blocks are translated.
    pub(super) fn finish_landings(&mut self) {
        let mut landings: Vec<(usize, ir::Block)> = self
            .landings
            .iter()
            .map(|(pad, block)| (*pad, *block))
            .collect();
        landings.sort_unstable_by_key(|(pad, _)| *pad);
        for (pad, block) in landings {
            self.builder.switch_to_block(block);
            let (error, context) = {
                let arriving = self.builder.block_params(block);
                (arriving[0], arriving[1])
            };
            self.builder.declare_value_needs_stack_map(error);
            self.builder.declare_value_needs_stack_map(context);
            if let Some(landing) = self.body.blocks[pad].landing {
                self.write_local(landing.error_local, error);
                self.write_local(landing.context_local, context);
            }
            let handler = self.blocks[pad];
            self.builder.ins().jump(handler, &[]);
        }
    }

    /// Mark what is emitted from here on as part of a `defer` body or not.
    pub(super) fn shield(&mut self, shielded: bool) {
        self.builder.set_srcloc(if shielded {
            SourceLoc::new(SHIELDED)
        } else {
            SourceLoc::default()
        });
    }

    /// End the block being generated by throwing `error`, a value in the
    /// representation of `unknown`, with `context` when it was caught with
    /// one: a jump to the handler when it is in this function, a call that
    /// does not return otherwise.
    pub(super) fn raise(
        &mut self,
        error: ir::Value,
        context: Option<ir::Value>,
    ) -> Result<(), CompileError> {
        if let Some(pad) = self.pad() {
            let landing = self.landing(pad);
            let context = match context {
                Some(context) => context,
                None => self.builder.ins().iconst(WORD, 0),
            };
            self.builder
                .ins()
                .jump(landing, &[error.into(), context.into()]);
            return Ok(());
        }
        let (function, context) = match context {
            Some(context) => (super::Runtime::Rethrow, context),
            // What the error was thrown during the handling of: nothing
            // reads it yet.
            None => (super::Runtime::Throw, self.builder.ins().iconst(WORD, 0)),
        };
        // Not through `call_runtime`: this path does not come back.
        self.emit_runtime_call(function, &[error, context])?;
        self.builder.ins().trap(TRAP_AFTER_PANIC);
        Ok(())
    }

    /// `throw value`.
    pub(super) fn throw(&mut self, value: &Operand<'db>) -> Result<(), CompileError> {
        let error = self.operand_as(value, &NativeTy::Unknown)?;
        self.raise(error, None)
    }

    /// Throw a caught error again with the context it was caught with.
    pub(super) fn rethrow(
        &mut self,
        value: &Operand<'db>,
        context: &Operand<'db>,
    ) -> Result<(), CompileError> {
        let error = self.operand_as(value, &NativeTy::Unknown)?;
        let context = self.operand_as(context, &NativeTy::Unknown)?;
        self.raise(error, Some(context))
    }

    /// Call a BAML function and return its result. From a block that
    /// unwinds to a handler the call carries the edge to it, and what
    /// follows is generated in the block the call returns to.
    pub(super) fn baml_call(&mut self, called: &Called, arguments: &[ir::Value]) -> ir::Value {
        let Some(pad) = self.pad() else {
            let call = match called {
                Called::Direct(function) => self.builder.ins().call(*function, arguments),
                Called::Indirect(signature, code) => self
                    .builder
                    .ins()
                    .call_indirect(*signature, *code, arguments),
            };
            return self.builder.inst_results(call)[0];
        };
        let landing = self.landing(pad);
        let wants_context = self.wants_context(pad);
        try_call(&mut self.builder, called, arguments, landing, wants_context)
    }
}
