//! Generates Rust bodies from MIR using the VM runtime and structured control flow.

mod call;
mod constant;
mod expr;
mod place;
mod stmt;
pub(crate) mod types;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
};

use baml_compiler2_mir::{
    BlockId, Constant, Local, MirFunction, MirFunctionBody, Operand, Place, Terminator,
    function_display_name,
};
use types::{realized_expr, template_blocker};

use crate::{
    admit::Rejection,
    analysis::{
        Analysis, Repr, Signature, array_iter_walk, array_next_call, reads_frame,
        suspends_directly, user_callee,
    },
    index::ProgramIndex,
    render::coerce,
    structure::{Structured, structurize},
};

struct Expr {
    code: String,
    repr: Repr,
    /// A local, literal or path: used as a call argument as it is.
    atom: bool,
}

impl Expr {
    fn new(code: impl Into<String>, repr: Repr) -> Self {
        Self {
            code: code.into(),
            repr,
            atom: false,
        }
    }

    fn atom(code: impl Into<String>, repr: Repr) -> Self {
        Self {
            code: code.into(),
            repr,
            atom: true,
        }
    }
}

pub(crate) struct ProgramContext<'a, 'db> {
    pub(crate) db: &'db dyn baml_compiler2_mir::Db,
    pub(crate) index: &'a ProgramIndex<'a>,
    pub(crate) symbols: &'a HashMap<usize, String>,
    /// Functions that may suspend, so a call to one roots the caller's live values.
    pub(crate) pausing: &'a BTreeSet<usize>,
    /// Functions whose bodies run with a VM frame, never a frame record.
    pub(crate) framed: &'a BTreeSet<usize>,
    /// Typed signatures of the direct entries of compiled bodies.
    pub(crate) signatures: &'a HashMap<usize, Signature>,
}

pub(crate) struct Generator<'a, 'db> {
    pub(crate) db: &'db dyn baml_compiler2_mir::Db,
    pub(crate) index: &'a ProgramIndex<'a>,
    /// Function object whose constant pool and package this body uses.
    pub(crate) object: usize,
    pub(crate) package: Option<u32>,
    pub(crate) symbols: &'a HashMap<usize, String>,
    pub(crate) pausing: &'a BTreeSet<usize>,
    /// Functions whose bodies run with a VM frame, never a frame record.
    pub(crate) framed: &'a BTreeSet<usize>,
    pub(crate) signatures: &'a HashMap<usize, Signature>,
    /// First spill slot used to root values across a suspension.
    pub(crate) spill_base: usize,
    /// Whether this body may suspend: it then polls at loop back edges and
    /// keeps live heap values in frame slots across every suspension.
    pub(crate) pauses: bool,
    pub(crate) recursive: bool,
    pub(crate) builtins: &'a mut BTreeMap<String, usize>,
    pub(crate) body: &'a MirFunctionBody<'db>,
    pub(crate) lambdas: &'a [MirFunction<'db>],
    pub(crate) analysis: &'a Analysis,
    pub(crate) positions: &'a bex_vm_types::Bytecode,
    /// Source location and shielding last recorded in this leaf.
    pub(crate) current_mark: Option<(usize, bool)>,
    /// Exact position for error branches; normal execution coalesces by line.
    pub(crate) current_pc: Option<usize>,
    pub(crate) out: String,
    /// Bind non-atomic operands before calls to avoid overlapping VM borrows.
    pub(crate) pending: Vec<String>,
    pub(crate) temps: usize,
    pub(crate) live_in: Vec<Vec<bool>>,
    pub(crate) slots: usize,
    /// Active handler context used by the VM cause chain.
    pub(crate) cause_slot: Option<usize>,
    pub(crate) cause_changes: Vec<bool>,
    pub(crate) call_types: Vec<String>,
    pub(crate) call_trace: Option<String>,
    pub(crate) structured: Option<Structured>,
    /// Static type value sites, by their generated expression; each site is
    /// one type object, as bytecode's `LoadType` site is.
    pub(crate) static_value_sites: Vec<String>,
}

type Gen<T> = Result<T, Rejection>;

impl<'a, 'db> Generator<'a, 'db> {
    pub(crate) fn new(
        program: &ProgramContext<'a, 'db>,
        builtins: &'a mut BTreeMap<String, usize>,
        object: usize,
        unit: &'a crate::admit::Unit<'db>,
    ) -> Self {
        Self {
            db: program.db,
            index: program.index,
            object,
            package: unit.package,
            symbols: program.symbols,
            pausing: program.pausing,
            framed: program.framed,
            signatures: program.signatures,
            spill_base: 0,
            pauses: program.pausing.contains(&object),
            recursive: unit.recursive,
            builtins,
            body: &unit.body,
            lambdas: &unit.lambdas,
            analysis: &unit.analysis,
            positions: program.index.positions(object),
            current_mark: None,
            current_pc: None,
            out: String::new(),
            pending: Vec::new(),
            temps: 0,
            live_in: Vec::new(),
            slots: 0,
            cause_slot: None,
            cause_changes: Vec::new(),
            call_types: Vec::new(),
            call_trace: None,
            structured: None,
            static_value_sites: Vec::new(),
        }
    }
}

/// Loop iterations between polls for a yield; see `Generator::loop_check`.
const YIELD_EVERY: u32 = 16;

/// A PC argument of generated code: the position's own, or the last one the
/// body set (`pc`) for a position the bytecode emitted nothing for.
fn pc_arg(pc: Option<usize>) -> String {
    pc.map_or_else(|| "pc".to_string(), |pc| pc.to_string())
}

fn unsupported<T>(what: impl Into<String>) -> Gen<T> {
    Err(Rejection::Unsupported(what.into()))
}

impl<'db> Generator<'_, 'db> {
    fn repr(&self, local: Local) -> Repr {
        self.analysis.reprs[local.0]
    }

    pub(crate) fn function(&mut self, symbol: &str) -> Gen<String> {
        if self.positions.block_pcs.is_empty() {
            return unsupported("bytecode without MIR positions (a deserialized program)");
        }
        if self.positions.block_pcs.len() != self.body.blocks.len()
            || self
                .positions
                .block_pcs
                .iter()
                .zip(&self.body.blocks)
                .any(|(pcs, block)| pcs.statements.len() != block.statements.len())
        {
            return Err(Rejection::Invalid(
                "the function's bytecode was emitted from other MIR".into(),
            ));
        }
        self.prepare();
        let structured = structurize(&self.analysis.cfg)
            .map_err(|error| Rejection::Invalid(error.to_string()))?;
        let stmts = structured.stmts.clone();
        self.structured = Some(structured);
        self.out.clear();
        let signature = self.analysis.signature(self.body);
        // The VM and runtime-dispatched calls enter with boxed arguments; a
        // compiled caller calls the typed direct entry.
        let params: Vec<String> = signature
            .params
            .iter()
            .enumerate()
            .map(|(i, repr)| coerce(&format!("args[{i}]"), Repr::Value, *repr))
            .collect();
        self.emit_line(&format!(
            "#[inline]\npub fn {symbol}(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> R<Value> {{"
        ));
        let passed = params.iter().fold(String::new(), |mut out, param| {
            let _ = write!(out, ", {param}");
            out
        });
        self.emit_line(&format!("let r = {symbol}_d(vm, fr{passed});"));
        self.emit_line(&format!("Ok({})", coerce("r?", signature.ret, Repr::Value)));
        self.emit_line("}");
        let ret = signature.ret.rust_type();
        let typed =
            signature
                .params
                .iter()
                .enumerate()
                .fold(String::new(), |mut out, (i, repr)| {
                    let _ = write!(out, ", arg{i}: {}", repr.rust_type());
                    out
                });
        // Every closure exit records the current PC before returning an error.
        self.emit_line(&format!(
            "#[inline]\npub fn {symbol}_d(vm: &mut BexVm, fr: &FrameRecord{typed}) -> R<{ret}> {{"
        ));
        self.emit_line("let mut pc: u32 = 0;");
        self.emit_line(&format!("let result = (|| -> R<{ret}> {{"));
        self.declare_locals(!self.analysis.loop_headers.is_empty());
        // The call path leaves the arguments in a hooked body's leading slots.
        if self.slots > self.slot_base() {
            self.emit_line(&format!("vm.reserve_slots(fr, {});", self.slots));
        }
        if let Some(slot) = self.cause_slot {
            self.emit_line(&format!("vm.set_slot(fr, {slot}, Value::NULL);"));
        }
        if self.recursive {
            self.recursion_check();
        }
        self.stmts(&stmts)?;
        // Every path ends in an `Exit`; this keeps the closure well-typed.
        self.emit_line("#[allow(unreachable_code)]");
        self.emit_line(&format!("Ok({})", signature.ret.default_value()));
        self.emit_line("})();");
        self.emit_line("if result.is_err() {");
        self.emit_line("fr.note_pc(pc);");
        self.emit_line("}");
        self.emit_line("result");
        self.emit_line("}");
        Ok(std::mem::take(&mut self.out))
    }

    /// Hook prologues need their arguments in leading slots across suspension.
    fn slot_base(&self) -> usize {
        if self.analysis.hooked {
            self.analysis.arity
        } else {
            0
        }
    }

    fn declare_locals(&mut self, iterations: bool) {
        let analysis = self.analysis;
        let arity = self.body.locals.len().min(self.analysis.arity);
        for (i, &repr) in analysis.reprs.iter().enumerate() {
            if analysis.is_type_local(i) {
                continue;
            }
            let ty = repr.rust_type();
            // Parameters arrive typed as `Signature` holds them.
            let init = if !(1..=arity).contains(&i) {
                repr.default_value().to_string()
            } else if self.body.locals[i].is_captured {
                format!("vm.new_cell(arg{})", i - 1)
            } else {
                format!("arg{}", i - 1)
            };
            self.emit_line(&format!("let mut l{i}: {ty} = {init};"));
        }
        for iter in &analysis.array_iters {
            self.emit_line(&format!("let mut i{iter}: i64 = 0;"));
        }
        if iterations {
            self.emit_line("let mut iterations: u32 = 0;");
        }
        for block in &self.body.blocks {
            let declaration = match &block.terminator {
                Some(
                    Terminator::Branch { .. }
                    | Terminator::NarrowBind { .. }
                    | Terminator::ShortCircuit { .. },
                ) => format!("let mut c{}: bool = false;", block.id.0),
                Some(Terminator::Switch { .. }) => format!("let mut d{}: i64 = 0;", block.id.0),
                _ => continue,
            };
            self.emit_line(&declaration);
        }
    }

    /// A realized type: built in place when static, bound to a temporary
    /// when it reads the frame, else read from the function's pooled template.
    pub(super) fn realized(&mut self, template: &baml_compiler2_mir::TyTemplate) -> Gen<String> {
        let frame = reads_frame(template);
        let Some(code) = realized_expr(self.index, template, frame) else {
            return self.pooled_type(template);
        };
        if !frame {
            return Ok(code);
        }
        Ok(self.bind_type(&code))
    }

    fn type_value(&mut self, template: &baml_compiler2_mir::TyTemplate) -> Gen<Expr> {
        let slot = match template {
            baml_compiler2_mir::TyTemplate::TypeArgRef(slot) => format!("Some({slot})"),
            _ => "None".to_string(),
        };
        let ty = self.realized(template)?;
        Ok(Expr::new(
            format!("vm.type_value(fr, {slot}, {ty})"),
            Repr::Value,
        ))
    }

    fn bind_type(&mut self, code: &str) -> String {
        let name = format!("t{}", self.temps);
        self.temps += 1;
        self.pending
            .push(format!("let {name}: RealizedTy = {code};"));
        name
    }

    /// Read a type from its pooled template with frame arguments substituted.
    fn pooled_type(&mut self, template: &baml_compiler2_mir::TyTemplate) -> Gen<String> {
        let blocked = || {
            unsupported(format!(
                "type template with {}",
                template_blocker(self.index, template)
            ))
        };
        let Some(anchored) = self.index.anchor(template) else {
            return blocked();
        };
        let Some(index) = self.index.pooled_constant(
            self.object,
            &bex_vm_types::types::ConstValue::Type(anchored),
        ) else {
            return blocked();
        };
        Ok(self.bind_type(&format!(
            "vm.frame_type_constant(fr, vm.record_object(fr, ObjectIndex::from_raw({})), {index})?",
            self.object
        )))
    }

    pub(super) fn realized_all(
        &mut self,
        templates: &[baml_compiler2_mir::TyTemplate],
    ) -> Gen<Vec<String>> {
        templates
            .iter()
            .map(|template| self.realized(template))
            .collect()
    }

    pub(super) fn realized_ref(
        &mut self,
        template: &baml_compiler2_mir::TyTemplate,
    ) -> Gen<String> {
        let owned = self.realized(template)?;
        Ok(format!("&{}", self.bind_type(&owned)))
    }

    /// Type locals rebuild their templates; other locals hold `reflect.Type` values.
    fn type_operand(&mut self, what: &str, operand: &Operand<'db>) -> Gen<String> {
        let local = match operand {
            Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => *local,
            Operand::Constant(_) | Operand::Copy(_) | Operand::Move(_) => {
                return unsupported(format!("{what} with a type argument that is not a local"));
            }
        };
        if let Some(ty) = self.analysis.static_types.get(&local.0) {
            return Ok(ty.code.clone());
        }
        if let Some(template) = self.analysis.deferred_types.get(&local.0) {
            return self.realized(template);
        }
        if self.analysis.array_iters.contains(&local.0) {
            return unsupported(format!("{what} with an array iterator as a type argument"));
        }
        let value = coerce(&format!("l{}", local.0), self.repr(local), Repr::Value);
        Ok(self.bind_type(&format!("vm.type_operand({value})?")))
    }

    fn prepare(&mut self) {
        let blocks = &self.body.blocks;
        let mut cause_changes = vec![None; blocks.len()];
        for block in blocks {
            for successor in block.terminator.iter().flat_map(Terminator::successors) {
                *cause_changes[successor.0].get_or_insert(false) |=
                    block.handling != blocks[successor.0].handling;
            }
        }
        self.cause_changes = cause_changes
            .into_iter()
            .zip(blocks)
            .map(|(change, block)| change.unwrap_or(block.handling.is_some()))
            .collect();
        let catches = blocks.iter().any(|block| block.handling.is_some());
        let base = self.slot_base();
        self.cause_slot = catches.then_some(base);
        self.slots = base
            + usize::from(catches)
            + blocks
                .iter()
                .filter(|block| block.landing.is_some())
                .count();
        self.spill_base = self.slots;
        if !self.pauses {
            return;
        }
        self.live_in = baml_compiler2_mir::live_in_precise(self.body);
        // Spill slots for the values live across a suspension: as many as the
        // widest site keeps.
        let at_sites = (0..blocks.len())
            .filter(|&block| self.continuation(BlockId(block)).is_ok())
            .filter_map(|block| self.rooted_locals(BlockId(block)).ok())
            .map(|live| live.len());
        let at_headers = self
            .analysis
            .loop_headers
            .iter()
            .map(|&header| self.header_locals(header).len());
        let at_entry = self
            .recursive
            .then(|| self.header_locals(self.body.entry.0).len());
        self.slots += at_sites
            .chain(at_headers)
            .chain(at_entry)
            .max()
            .unwrap_or(0);
    }

    fn recursion_check(&mut self) {
        let live = self.header_locals(self.body.entry.0);
        let call = self.rooted(&live, "vm.suspend(Step::Yield)");
        self.emit_line(&format!("if vm.should_yield_at_call() {{ {call}?; }}"));
    }

    fn header_locals(&self, header: usize) -> Vec<usize> {
        self.saved(|i| self.live_in[header][i])
    }

    /// Root the caller's live values across `call`, a suspending callee or
    /// request, reloading them whatever its outcome.
    pub(super) fn rooted_call(&mut self, block: BlockId, call: &str) -> Gen<String> {
        let live = self.rooted_locals(block)?;
        let call = self.rooted(&live, call);
        self.fallible(block, &call)
    }

    /// `call` with `live` written to the spill slots before and read back after.
    fn rooted(&self, live: &[usize], call: &str) -> String {
        if live.is_empty() {
            return call.to_string();
        }
        let mut out = String::from("{ ");
        for (j, i) in live.iter().enumerate() {
            let _ = write!(out, "vm.set_slot(fr, {}, l{i}); ", self.spill_base + j);
        }
        let _ = write!(out, "let r = {call}; ");
        for (j, i) in live.iter().enumerate() {
            let _ = write!(out, "l{i} = vm.slot(fr, {}); ", self.spill_base + j);
        }
        out.push_str("r }");
        out
    }

    /// Keep what the continuation reads except the result, plus what the
    /// handler reads.
    fn rooted_locals(&self, block: BlockId) -> Gen<Vec<usize>> {
        let (destination, target) = self.continuation(block)?;
        let overwritten = match destination {
            Place::Local(local) => Some(local.0),
            _ => None,
        };
        let handler = self.body.blocks[block.0].unwind;
        Ok(self.saved(|i| {
            (self.live_in[target.0][i] && overwritten != Some(i))
                || handler.is_some_and(|handler| self.live_in[handler.0][i])
        }))
    }

    /// Handler contexts stay live so leaving a nested handler can restore its cause.
    fn saved(&self, live: impl Fn(usize) -> bool) -> Vec<usize> {
        let kept = |i| {
            live(i)
                || (self.cause_slot.is_some()
                    && self
                        .body
                        .blocks
                        .iter()
                        .filter_map(|block| block.landing)
                        .any(|landing| landing.context_local.0 == i))
        };
        (0..self.analysis.reprs.len())
            .filter(|&i| {
                kept(i) && !self.analysis.is_type_local(i) && self.repr(Local(i)) == Repr::Value
            })
            .collect()
    }

    pub(super) fn suspension(&mut self, block: BlockId) -> Gen<()> {
        let terminator = self.body.blocks[block.0]
            .terminator
            .as_ref()
            .expect("checked");
        let (destination, _) = self.continuation(block)?;
        let request = self.request(block, terminator)?;
        let call = self.rooted_call(block, &request)?;
        let store = self.store(block, &destination, &Expr::new(call, Repr::Value))?;
        self.emit_statement(&store);
        Ok(())
    }

    /// An `R<Value>` expression that suspends with `step` after pushing `args`
    /// and the pending call types.
    fn suspend_expr(&mut self, block: BlockId, step: &str, args: &[String]) -> String {
        let mut out = String::from("{ ");
        for arg in args {
            let _ = write!(out, "vm.push_arg({arg}); ");
        }
        for ty in std::mem::take(&mut self.call_types) {
            let _ = write!(out, "vm.push_type_arg({ty}); ");
        }
        let site = self.site_pc(block);
        let _ = write!(out, "fr.note_pc({site}); vm.suspend({step}) }}");
        self.attach_trace(&out)
    }

    fn request(&mut self, block: BlockId, terminator: &Terminator<'db>) -> Gen<String> {
        Ok(match terminator {
            Terminator::Call {
                callee,
                args,
                ntypeargs,
                argument_layout,
                has_trace,
                ..
            } => {
                let (type_args, args) = args.split_at((*ntypeargs).min(args.len()));
                let types = self.type_operands("call", type_args)?;
                let plain = types.is_empty() && !*has_trace;
                self.call_types = types;
                let named = argument_layout
                    .as_ref()
                    .is_some_and(|layout| layout.0.iter().any(Option::is_some));
                if let Operand::Constant(Constant::Function(func)) = callee {
                    let args = self.trace_attachment(block, args, *has_trace)?;
                    self.callee_ptr(block, callee, None)?;
                    let name = function_display_name(self.db, *func);
                    let Some((_, info)) = self.index.function(self.db, *func) else {
                        return unsupported(format!("callee `{name}` not found in the program"));
                    };
                    let values =
                        self.laid_out_args(block, &name, info, argument_layout.as_ref(), args)?;
                    let step = format!("Step::Call {{ callee, argc: {} }}", values.len());
                    return Ok(self.suspend_expr(block, &step, &values));
                }
                let (args, trace) = if *has_trace {
                    let Some((trace, args)) = args.split_last() else {
                        return Err(Rejection::Invalid(
                            "a traced call without its attachment".into(),
                        ));
                    };
                    (args, Some(trace))
                } else {
                    (args, None)
                };
                // Bytecode pushes the arguments before loading an indirect callee.
                let values = self.arg_values(block, args)?;
                self.callee_ptr(block, callee, trace)?;
                // Function values need their named arguments mapped at runtime.
                if named {
                    let layout = argument_layout.as_ref().expect("checked");
                    let slots = call::named_arg_slots(layout);
                    self.emit_statement(&format!(
                        "let argc = vm.push_indirect_args(callee, &[{}], &[{}])?;",
                        slots,
                        values.join(", ")
                    ));
                    return Ok(self.suspend_expr(block, "Step::Call { callee, argc }", &[]));
                }
                let step = format!("Step::Call {{ callee, argc: {} }}", values.len());
                let suspend = self.suspend_expr(block, &step, &values);
                if !plain {
                    return Ok(suspend);
                }
                // A compiled closure runs in place.
                let site = self.site_pc(block);
                let values = values.join(", ");
                format!(
                    "match vm.closure_compiled_body(callee, {}) {{ \
             Some(body) => vm.call_closure_compiled(fr, callee, body, &[{values}], {site}), \
             None => {suspend} }}",
                    args.len()
                )
            }
            Terminator::SysOp { callee, args, .. } => {
                self.callee_ptr(block, callee, None)?;
                let args = self.arg_values(block, args)?;
                let step = format!("Step::Call {{ callee, argc: {} }}", args.len());
                self.suspend_expr(block, &step, &args)
            }
            Terminator::VirtualCall {
                iface,
                method,
                args,
                ntypeargs,
                self_arg,
                argument_layout,
                has_trace,
                ..
            } => {
                let args = self.trace_attachment(block, args, *has_trace)?;
                self.virtual_request(
                    block,
                    iface,
                    method,
                    args,
                    *has_trace,
                    *ntypeargs,
                    *self_arg,
                    argument_layout.as_ref(),
                )?
            }
            Terminator::Await { future, .. } => {
                let future = self.load(block, future)?;
                let future = self.value_arg(&future);
                self.emit_statement(&format!("let operand = {future};"));
                self.suspend_expr(block, "Step::Await(operand)", &[])
            }
            Terminator::AwaitAny { futures, .. } => {
                let futures = self.value_operand(block, futures)?;
                self.emit_statement(&format!("let operand = {futures};"));
                self.suspend_expr(block, "Step::AwaitAny(operand)", &[])
            }
            Terminator::Spawn { plan, .. } => {
                let plan = self.value_operand(block, plan)?;
                self.emit_statement(&format!("let operand = {plan};"));
                self.suspend_expr(block, "Step::Spawn(operand)", &[])
            }
            _ => {
                return Err(Rejection::Invalid(
                    "suspension at a non-suspending block".into(),
                ));
            }
        })
    }

    fn trace_attachment<'b>(
        &mut self,
        block: BlockId,
        args: &'b [Operand<'db>],
        traced: bool,
    ) -> Gen<&'b [Operand<'db>]> {
        if !traced {
            return Ok(args);
        }
        let Some((trace, args)) = args.split_last() else {
            return Err(Rejection::Invalid(
                "a traced call without its attachment".into(),
            ));
        };
        let trace = self.value_operand(block, trace)?;
        self.call_trace = Some(trace);
        Ok(args)
    }

    fn attach_trace(&mut self, call: &str) -> String {
        self.call_trace.take().map_or_else(
            || call.to_string(),
            |trace| format!(
                "match vm.set_call_trace({trace}) {{ Ok(()) => {call}, Err(e) => {{ vm.reserve_slots(fr, {}); Err(e) }} }}",
                self.slots
            ),
        )
    }

    fn continuation(&self, block: BlockId) -> Gen<(Place, BlockId)> {
        self.body.blocks[block.0]
            .terminator
            .as_ref()
            .and_then(crate::analysis::continuation)
            .map(|(destination, target)| (destination.clone(), target))
            .ok_or_else(|| Rejection::Invalid("continuation of a non-suspending block".into()))
    }

    fn emit_line(&mut self, text: &str) {
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn emit_statement(&mut self, text: &str) {
        self.flush_bindings();
        self.emit_line(text);
    }

    /// VM call arguments must be atoms to avoid overlapping mutable borrows.
    fn hoist(&mut self, expr: Expr) -> Expr {
        if expr.atom {
            return expr;
        }
        let name = format!("t{}", self.temps);
        self.temps += 1;
        self.pending.push(format!(
            "let {name}: {} = {};",
            expr.repr.rust_type(),
            expr.code
        ));
        Expr::atom(name, expr.repr)
    }

    fn value_operand(&mut self, block: BlockId, operand: &Operand<'db>) -> Gen<String> {
        let operand = self.operand(block, operand)?;
        Ok(self.value_arg(&operand))
    }

    /// `expr` as an atomic `Value` argument.
    fn value_arg(&mut self, expr: &Expr) -> String {
        if expr.atom && expr.repr == Repr::Value {
            return expr.code.clone();
        }
        let coerced = coerce(&expr.code, expr.repr, Repr::Value);
        self.hoist(Expr::new(coerced, Repr::Value)).code
    }

    fn flush_bindings(&mut self) {
        for binding in std::mem::take(&mut self.pending) {
            self.emit_line(&binding);
        }
    }

    /// Record a PC only when its source location or shielding changes.
    /// Positions without bytecode retain the last PC.
    fn set_pc(&mut self, pc: Option<usize>) {
        let Some(pc) = pc else { return };
        self.current_pc = Some(pc);
        let mark = (
            self.positions.source_line_for_pc(pc),
            self.positions.pc_is_shielded(pc),
        );
        if self.current_mark != Some(mark) {
            self.current_mark = Some(mark);
            self.emit_line(&format!("pc = {pc};"));
        }
    }

    fn site_pc(&self, block: BlockId) -> String {
        pc_arg(
            self.positions
                .block_pcs
                .get(block.0)
                .and_then(|pcs| pcs.terminator),
        )
    }

    /// Array iteration is inline; other virtual calls can suspend.
    pub(super) fn array_iteration(&self, terminator: &Terminator<'db>) -> bool {
        array_iter_walk(
            self.body,
            self.analysis.arity,
            terminator,
            &self.analysis.array_iters,
        )
        .is_some()
            || array_next_call(terminator, &self.analysis.array_iters).is_some()
    }

    /// Calls the VM must make: anything but a compiled callee entered directly.
    fn call_suspends(&self, terminator: &Terminator<'db>) -> bool {
        if suspends_directly(
            self.db,
            self.index,
            self.body,
            terminator,
            &self.analysis.static_types,
            &self.analysis.hook_results,
        ) {
            return true;
        }
        let Terminator::Call { callee, .. } = terminator else {
            return false;
        };
        user_callee(self.db, self.index, callee)
            .is_some_and(|object| !self.symbols.contains_key(&object))
    }
}
