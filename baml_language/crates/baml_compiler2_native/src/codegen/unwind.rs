//! Error unwinding: the table the runtime's unwinder finds handlers in, and
//! the code a throw, a rethrow and a call under a handler compile to.
//!
//! A frame with no handler pays nothing: its calls are plain calls and it
//! has no entry in the table. A call of a BAML function from a block that
//! unwinds to a handler is a `try_call` with an exception edge to that
//! handler and to each handler around it in the function, and the table
//! records them by the call's return address, with what each one takes. A
//! throw whose handler is in the same function is a jump to it. Any other
//! throw calls the runtime, which walks the frame records and resumes in
//! the first handler that takes the error, with the error and its trace in
//! the two payload registers.
//!
//! What a handler takes is read off its MIR: a `catch` lowers to a chain of
//! type tests on the error it landed with, ending in a rethrow for what no
//! arm takes. The unwinder is told the outcome of that chain, so it never
//! lands in a handler only for the chain to throw the error on. The chain
//! stays in the code: a throw in the handler's own function jumps to it.

use std::collections::HashMap;

use baml_compiler2_mir::{
    BlockId, Local, Operand, Place, Rvalue, StatementKind, SwitchKey, Terminator,
};
use cranelift_codegen::{
    CompiledCode, FinalizedMachExceptionHandler,
    ir::{
        self, BlockArg, ExceptionTableData, ExceptionTableItem, ExceptionTag, InstBuilder,
        SourceLoc,
    },
};
use cranelift_module::{DataDescription, FuncId, Linkage, Module};

use super::{Backend, TRAP_AFTER_PANIC, Translator, WORD, internal};
use crate::{
    CompileError, layout,
    narrow::TestKind,
    types::{NativeTy, binds_context},
};

/// The symbol of the table of call sites that have a handler.
pub(crate) const UNWIND_SYMBOL: &str = "baml_native_unwind";
/// The version of that table's format.
const UNWIND_VERSION: u32 = 2;

/// Marks the instructions of a `defer` body, so that the ranges of code
/// they become can be read back from the compiled function.
const SHIELDED: u32 = 1;

/// What a handler takes, and what it does with it; see
/// [`layout::HANDLER_TAKES_ALL`] and the flags after it.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Catch {
    flags: u32,
    /// The type ids it takes whatever its flags say.
    ids: Vec<u32>,
}

impl Catch {
    /// What the handler of an entry the runtime calls takes: every error,
    /// to hand it to the runtime, which has compiled code throw it again
    /// further out. Whether its trace is wanted is for the handlers out
    /// there to say.
    pub(super) fn handing_on() -> Self {
        Self {
            flags: layout::HANDLER_TAKES_ALL | layout::HANDLER_MAY_RETHROW,
            ids: Vec::new(),
        }
    }
}

/// A call with handlers.
pub(super) struct Site {
    /// The offset in the function of the call's return address.
    return_offset: u32,
    /// The frame pointer minus the stack pointer at the call.
    frame_offset: u32,
    /// Its handlers among the function's, innermost first.
    first_handler: u32,
    handlers: u32,
}

/// The sites of one function and the handlers they name: each the offset
/// in the function of its first instruction and what it takes.
struct Handled {
    function: FuncId,
    sites: Vec<Site>,
    handlers: Vec<(u32, Catch)>,
}

/// What every function compiled so far contributes to the unwinder's
/// tables.
#[derive(Default)]
pub(super) struct Tables {
    handled: Vec<Handled>,
    /// What the handlers of the function being compiled take, by the tag
    /// its exception edges carry.
    pub(super) catches: HashMap<u32, Catch>,
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
        let catches = std::mem::take(&mut self.catches);
        let mut sites = Vec::new();
        let mut handlers: Vec<(u32, Catch)> = Vec::new();
        for site in code.buffer.call_sites() {
            let mut chain = Vec::new();
            for handler in site.exception_handlers {
                match handler {
                    FinalizedMachExceptionHandler::Tag(tag, offset) => {
                        let catch = catches
                            .get(&tag.as_u32())
                            .ok_or_else(|| internal("an exception edge names a handler"))?;
                        chain.push((*offset, catch.clone()));
                    }
                    // The function that enters compiled code: it takes
                    // everything, to report it with its trace.
                    FinalizedMachExceptionHandler::Default(offset) => chain.push((
                        *offset,
                        Catch {
                            flags: layout::HANDLER_TAKES_ALL | layout::HANDLER_BINDS_CONTEXT,
                            ids: Vec::new(),
                        },
                    )),
                    FinalizedMachExceptionHandler::Context(_) => {}
                }
            }
            if chain.is_empty() {
                continue;
            }
            // Calls under the same handlers share their records.
            let first = handlers
                .windows(chain.len())
                .position(|window| window == chain)
                .unwrap_or_else(|| {
                    handlers.extend(chain.iter().cloned());
                    handlers.len() - chain.len()
                });
            sites.push(Site {
                return_offset: site.ret_addr,
                frame_offset: site
                    .frame_offset
                    .ok_or_else(|| internal("a call with a handler says where its frame is"))?,
                first_handler: u32::try_from(first).map_err(internal)?,
                handlers: u32::try_from(chain.len()).map_err(internal)?,
            });
        }
        if !sites.is_empty() {
            self.handled.push(Handled {
                function: id,
                sites,
                handlers,
            });
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
    /// Emit the table of calls that have handlers. Every field is
    /// little-endian and the table is aligned to 8 bytes:
    ///
    /// ```text
    /// +0   u32  format version (2)
    /// +4   u32  F, the number of functions that have such a call
    /// then for each function:
    ///        +0   u64  address of the function's first instruction
    ///        +8   u32  N, the number of its calls with handlers
    ///        +12  u32  H, the number of its handler records
    ///        +16  N site records of 16 bytes:
    ///               +0   u32  offset in the function of the call's return
    ///                         address
    ///               +4   u32  the frame pointer minus the stack pointer
    ///                         at the call
    ///               +8   u32  which of the function's handler records is
    ///                         its innermost handler's
    ///               +12  u32  how many handlers it has: their records
    ///                         follow that one, each the handler around
    ///                         the one before it
    ///        then H handler records of 16 bytes:
    ///               +0   u32  offset in the function of the handler
    ///               +4   u32  flags: 1 takes every error, 2 takes every
    ///                         panic, 4 takes every error that is not a
    ///                         panic, 8 takes `null`, 16 reads the context
    ///                         it lands with, 32 may throw the error again
    ///               +8   u32  offset in the table of the type ids it
    ///                         takes besides, a `u32` each
    ///               +12  u32  how many type ids
    /// then the type ids the handler records locate
    /// ```
    pub(super) fn define_unwind(&mut self) -> Result<(), CompileError> {
        let functions = std::mem::take(&mut self.unwind.handled);
        let mut description = DataDescription::new();
        let mut bytes: Vec<u8> = Vec::new();
        let count = u32::try_from(functions.len()).map_err(internal)?;
        for word in [UNWIND_VERSION, count] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        let records: usize = functions
            .iter()
            .map(|function| 16 + 16 * (function.sites.len() + function.handlers.len()))
            .sum();
        let mut ids: Vec<u32> = Vec::new();
        let first_id = bytes.len() + records;
        for function in functions {
            let address = self
                .module
                .declare_func_in_data(function.function, &mut description);
            description.write_function_addr(u32::try_from(bytes.len()).map_err(internal)?, address);
            bytes.extend_from_slice(&0_u64.to_le_bytes());
            let sites = u32::try_from(function.sites.len()).map_err(internal)?;
            let handlers = u32::try_from(function.handlers.len()).map_err(internal)?;
            for word in [sites, handlers] {
                bytes.extend_from_slice(&word.to_le_bytes());
            }
            for site in function.sites {
                for word in [
                    site.return_offset,
                    site.frame_offset,
                    site.first_handler,
                    site.handlers,
                ] {
                    bytes.extend_from_slice(&word.to_le_bytes());
                }
            }
            for (offset, catch) in function.handlers {
                let listed = u32::try_from(first_id + 4 * ids.len()).map_err(internal)?;
                let count = u32::try_from(catch.ids.len()).map_err(internal)?;
                for word in [offset, catch.flags, listed, count] {
                    bytes.extend_from_slice(&word.to_le_bytes());
                }
                ids.extend(catch.ids);
            }
        }
        debug_assert_eq!(bytes.len(), first_id);
        for id in ids {
            bytes.extend_from_slice(&id.to_le_bytes());
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

/// A `try_call` of `called` with an exception edge to each of `handlers`,
/// innermost first, and the block the call returns to, which receives its
/// result. A handler is named by the tag its edge carries; the one without
/// a tag is the function's that enters compiled code.
///
/// Each edge lands in a block of its own that only jumps to the handler. An
/// argument of the `try_call`'s own edge is read before the call, so a
/// reference passed there would be the one from before any collection in
/// the callee; as an argument of the jump it is read after the call, from
/// the slot the collector updated.
pub(super) fn try_call(
    builder: &mut cranelift_frontend::FunctionBuilder<'_>,
    called: &Called,
    arguments: &[ir::Value],
    handlers: &[(Option<u32>, ir::Block)],
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
    let landed: Vec<ir::Block> = handlers
        .iter()
        .map(|_| {
            let landed = builder.create_block();
            builder.append_block_param(landed, WORD);
            builder.append_block_param(landed, WORD);
            builder.set_cold_block(landed);
            landed
        })
        .collect();
    let edges: Vec<ExceptionTableItem> = handlers
        .iter()
        .zip(&landed)
        .map(|((tag, _), landed)| {
            let thrown = builder
                .func
                .dfg
                .block_call(*landed, &[BlockArg::TryCallExn(0), BlockArg::TryCallExn(1)]);
            match tag {
                Some(tag) => ExceptionTableItem::Tag(ExceptionTag::from_u32(*tag), thrown),
                None => ExceptionTableItem::Default(thrown),
            }
        })
        .collect();
    let table = builder
        .func
        .dfg
        .exception_tables
        .push(ExceptionTableData::new(signature, normal, edges));
    match called {
        Called::Direct(function) => builder.ins().try_call(*function, arguments, table),
        Called::Indirect(_, code) => builder.ins().try_call_indirect(*code, arguments, table),
    };
    for ((_, landing), landed) in handlers.iter().zip(landed) {
        builder.seal_block(landed);
        builder.switch_to_block(landed);
        let error = builder.block_params(landed)[0];
        let context = builder.block_params(landed)[1];
        builder
            .ins()
            .jump(*landing, &[error.into(), context.into()]);
    }
    builder.seal_block(returned);
    builder.switch_to_block(returned);
    builder.block_params(returned)[0]
}

/// The local an operand reads whole.
fn read_local(operand: &Operand<'_>) -> Option<Local> {
    match operand {
        Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => Some(*local),
        _ => None,
    }
}

impl<'db> Translator<'_, 'db> {
    /// The handler a throw in the block being translated unwinds to, when
    /// it is in this function.
    fn pad(&self) -> Option<BlockId> {
        self.body.blocks[self.current].unwind
    }

    /// The block a throw to the handler `pad` jumps to with the error and
    /// its trace, which is zero when nothing will read one: it puts them in the handler's locals and goes on to
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

    /// What the handler `pad` takes, and the handler around it in this
    /// function: where an error it does not take goes on to.
    ///
    /// A `catch` starts with a chain of blocks, each one test of the error
    /// it landed with and a branch on it: to an arm, or to a block that
    /// only throws the error again, which is how a handler that takes every
    /// error lets a panic pass and how one with no arm for an error passes
    /// it on. What the chain ends in takes whatever reaches it. A handler
    /// whose chain this cannot read takes everything, and tests it where
    /// it lands.
    fn catch(&mut self, pad: BlockId) -> Result<(Catch, Option<BlockId>), CompileError> {
        let body = self.body;
        let Some(landing) = body.blocks[pad.0].landing else {
            return Err(internal("a handler lands with an error"));
        };
        let (error, context) = (landing.error_local, landing.context_local);
        let rethrows = |block: usize| {
            matches!(
                &body.blocks[block].terminator,
                Some(Terminator::Rethrow { context: thrown, .. }) if read_local(thrown) == Some(context)
            )
        };
        // A block that throws on what the handler landed with and does
        // nothing else.
        let passes = |block: BlockId| {
            let block = &body.blocks[block.0];
            block
                .statements
                .iter()
                .all(|statement| matches!(statement.kind, StatementKind::Nop))
                && matches!(
                    &block.terminator,
                    Some(Terminator::Rethrow { value, context: thrown })
                        if read_local(value) == Some(error) && read_local(thrown) == Some(context)
                )
        };

        let mut flags = 0;
        let mut ids: Vec<usize> = Vec::new();
        let mut passed: Vec<BlockId> = Vec::new();
        // Whether panics have been passed on by the tests so far.
        let mut panics_pass = false;
        let mut unreadable = false;
        let mut block = pad;
        loop {
            if passes(block) {
                passed.push(block);
                break;
            }
            let (statements, terminator) = (
                &body.blocks[block.0].statements,
                &body.blocks[block.0].terminator,
            );
            // One test of the error and a branch on it, or one read of its
            // type tag and a switch on that.
            let tested = match (&statements[..], terminator) {
                (
                    [statement],
                    Some(Terminator::Switch {
                        discriminant,
                        arms,
                        otherwise,
                        ..
                    }),
                ) if matches!(
                    &statement.kind,
                    StatementKind::Assign {
                        destination: Place::Local(tag),
                        value: Rvalue::TypeTag(Place::Local(read)),
                    } if read_local(discriminant) == Some(*tag) && *read == error
                ) && !arms.iter().any(|(_, target)| passes(*target)) =>
                {
                    arms.iter()
                        .map(|(key, _)| match key {
                            SwitchKey::Class(class) => {
                                Some(TestKind::Exactly(NativeTy::Class(*class, Vec::new())))
                            }
                            SwitchKey::Int(tag) => TestKind::of_tag(*tag),
                        })
                        .collect::<Option<Vec<_>>>()
                        .map(|tests| (TestKind::AnyOf(tests), None, *otherwise))
                }
                (
                    [statement],
                    Some(Terminator::Branch {
                        condition,
                        then_block,
                        else_block,
                    }),
                ) => match &statement.kind {
                    StatementKind::Assign {
                        destination: Place::Local(result),
                        value,
                    } if read_local(condition) == Some(*result) => {
                        let test = match value {
                            Rvalue::IsType { operand, test }
                                if read_local(operand) == Some(error) =>
                            {
                                TestKind::of(self.db, test, body.arguments)
                            }
                            Rvalue::IsTypeTag { operand, tag }
                                if read_local(operand) == Some(error) =>
                            {
                                TestKind::of_tag(*tag)
                            }
                            _ => None,
                        };
                        // An interface's implementors are a list of types too.
                        test.filter(|test| {
                            test.is_static()
                                || matches!(test, TestKind::Panic | TestKind::Implements(_))
                        })
                        .map(|test| (test, Some(*then_block), *else_block))
                    }
                    _ => None,
                },
                _ => None,
            };
            let Some((test, taken, next)) = tested else {
                // The rest of the handler: an arm that takes whatever
                // reaches it, a `defer` body, or tests this does not read.
                flags |= if panics_pass {
                    layout::HANDLER_TAKES_OTHERS
                } else {
                    layout::HANDLER_TAKES_ALL
                };
                break;
            };
            let passing = self.passing(&test)?;
            if let Some(taken) = taken.filter(|taken| passes(*taken)) {
                // Only panics are passed on by a test of their own.
                if !passing.panics || !passing.ids.is_empty() || passing.null || panics_pass {
                    unreadable = true;
                    break;
                }
                panics_pass = true;
                passed.push(taken);
            } else {
                if passing.panics && !panics_pass {
                    flags |= layout::HANDLER_TAKES_PANICS;
                }
                if passing.null {
                    flags |= layout::HANDLER_TAKES_NULL;
                }
                for id in passing.ids {
                    if !(panics_pass && self.is_panic_id(id)?) {
                        ids.push(id);
                    }
                }
            }
            block = next;
        }

        // Where the error goes on to: the handler of the blocks that throw
        // it again, which are all inside the same handlers.
        let mut around: Option<Option<BlockId>> = None;
        let mut may_rethrow = false;
        for index in (0..body.blocks.len()).filter(|index| rethrows(*index)) {
            let outer = body.blocks[index].unwind;
            if around.is_some_and(|around| around != outer) {
                unreadable = true;
            }
            around = Some(outer);
            may_rethrow |= !passed.contains(&BlockId(index));
        }
        if unreadable {
            // Landed in for every error, and taken to read the trace so
            // that nothing is decided past it.
            return Ok((
                Catch {
                    flags: layout::HANDLER_TAKES_ALL | layout::HANDLER_BINDS_CONTEXT,
                    ids: Vec::new(),
                },
                None,
            ));
        }
        if may_rethrow {
            flags |= layout::HANDLER_MAY_RETHROW;
        }
        if binds_context(body, landing) {
            flags |= layout::HANDLER_BINDS_CONTEXT;
        }
        ids.sort_unstable();
        ids.dedup();
        let ids = ids
            .into_iter()
            .map(|id| u32::try_from(id).map_err(internal))
            .collect::<Result<_, _>>()?;
        Ok((Catch { flags, ids }, around.flatten()))
    }

    /// The exception edges of a call in a block that unwinds to `pad`: to
    /// its landing and to that of each handler around it, with the tag the
    /// table knows each by.
    fn handlers(&mut self, pad: BlockId) -> Result<Vec<(Option<u32>, ir::Block)>, CompileError> {
        let mut edges = Vec::new();
        let mut next = Some(pad);
        while let Some(pad) = next {
            let tag = u32::try_from(pad.0).map_err(internal)?;
            if edges.iter().any(|(seen, _)| *seen == Some(tag)) {
                return Err(internal("a handler is not inside itself"));
            }
            let around = match self.chains.get(&pad.0) {
                Some(around) => *around,
                None => {
                    let (catch, around) = self.catch(pad)?;
                    self.backend.unwind.catches.insert(tag, catch);
                    self.chains.insert(pad.0, around);
                    around
                }
            };
            edges.push((Some(tag), self.landing(pad)));
            next = around;
        }
        Ok(edges)
    }

    /// Give every landing block its code. Called once the function's own
    /// blocks are translated.
    pub(super) fn finish_landings(&mut self) -> Result<(), CompileError> {
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
            // A handler that reads its context reads a `baml.errors.Context`:
            // made here, of the trace it landed with.
            let binds = self.body.blocks[pad]
                .landing
                .is_some_and(|landing| binds_context(self.body, landing));
            let context = if binds {
                let mutator = self.mutator();
                let made = self
                    .emit_runtime_call(super::Runtime::ErrorContext, &[mutator, error, context])?
                    .ok_or_else(|| internal("the runtime returns the context it makes"))?;
                self.builder.declare_value_needs_stack_map(made);
                made
            } else {
                context
            };
            if let Some(landing) = self.body.blocks[pad].landing {
                self.write_local(landing.error_local, error);
                self.write_local(landing.context_local, context);
            }
            let handler = self.blocks[pad];
            self.builder.ins().jump(handler, &[]);
        }
        Ok(())
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
    pub(super) fn baml_call(
        &mut self,
        called: &Called,
        arguments: &[ir::Value],
    ) -> Result<ir::Value, CompileError> {
        let Some(pad) = self.pad() else {
            let call = match called {
                Called::Direct(function) => self.builder.ins().call(*function, arguments),
                Called::Indirect(signature, code) => self
                    .builder
                    .ins()
                    .call_indirect(*signature, *code, arguments),
            };
            return Ok(self.builder.inst_results(call)[0]);
        };
        let handlers = self.handlers(pad)?;
        Ok(try_call(&mut self.builder, called, arguments, &handlers))
    }
}
