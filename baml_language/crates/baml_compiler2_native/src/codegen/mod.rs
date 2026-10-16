//! MIR to Rust, against the VM's runtime surface.
//!
//! One Rust function per admitted BAML function, with the VM's native call
//! signature. Statically typed `int`, `float`, and `bool` locals are raw
//! `i64`, `f64`, and `bool`; every other local is a `Value`. Every operation
//! is a call into `bex_vm::compiled_rt`, the implementation the interpreter
//! runs too, so results and exceptions are identical by construction.
//!
//! Control flow is structured by [`crate::structure`] from the MIR graph, so
//! loops are Rust loops and LLVM sees them as such. A block under a `catch`
//! has an unwind edge; a fallible operation in such a block binds the thrown
//! value into the handler's error local and jumps there, exactly as the
//! interpreter's unwinder would within one frame.

mod call;
mod constant;
mod expr;
mod place;
mod stmt;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
};

use baml_compiler2_mir::{
    BlockId, Constant, Local, MirFunctionBody, Operand, Place, StatementKind, SwitchKey,
    Terminator, function_display_name,
};

use crate::{
    admit::{ProgramIndex, Rejection},
    analysis::{Analysis, Repr, loops, suspends_directly, user_callee_name},
    render::{coerce, is_atom},
    structure::{Structured, structurize},
};

/// A generated expression and how it is held.
struct Expr {
    code: String,
    repr: Repr,
}

impl Expr {
    fn new(code: impl Into<String>, repr: Repr) -> Self {
        Self {
            code: code.into(),
            repr,
        }
    }
}

pub(crate) struct Generator<'a, 'db> {
    pub(crate) db: &'db dyn baml_compiler2_mir::Db,
    pub(crate) index: &'a ProgramIndex,
    pub(crate) symbols: &'a HashMap<String, String>,
    /// The admitted functions that can suspend.
    pub(crate) resumable: &'a BTreeSet<String>,
    /// Whether this function can suspend: it is emitted as a resumable state
    /// machine over its blocks rather than as structured code.
    pub(crate) suspends: bool,
    /// While emitting a resumable function's block loop (where a jump is
    /// `bb = N; continue`), as opposed to its resume prologue.
    pub(crate) in_loop: bool,
    /// Builtin native keys the module binds, by key → ordinal.
    pub(crate) natives: &'a mut BTreeMap<String, usize>,
    pub(crate) body: &'a MirFunctionBody<'db>,
    pub(crate) analysis: &'a Analysis,
    /// Line starts of the source file, and the function's declaration span.
    pub(crate) lines: &'a [u32],
    pub(crate) declared_at: Option<baml_base::Span>,
    /// The line the code emitted so far in the current leaf has set.
    pub(crate) current_line: Option<usize>,
    pub(crate) out: String,
    /// `let` bindings to emit before the statement being generated: every
    /// operand that is not an atom is bound first, so no call's argument list
    /// holds another call (two `vm` borrows) whatever the expression is.
    pub(crate) pending: Vec<String>,
    pub(crate) temps: usize,
    /// For a resumable function, the locals live on entry to each block:
    /// what a suspension keeps in the frame's slots.
    pub(crate) live_in: Vec<Vec<bool>>,
    /// The stack slots the body keeps in its frame: a resumable body's
    /// locals and iterator positions, then its point slot, then the cause
    /// slot; just the cause slot for an inline body with a `catch`.
    pub(crate) slots: usize,
    /// The slot holding the context of the error being handled, which the
    /// VM's cause chain reads; `None` without a `catch`.
    pub(crate) cause_slot: Option<usize>,
    /// A resumable body's slot for the point it suspended at.
    pub(crate) point_slot: usize,
    /// Each block's predecessors along normal edges.
    pub(crate) preds: Vec<Vec<usize>>,
    /// A resumable body's loop headers: where it checks for a yield.
    pub(crate) loop_headers: BTreeSet<usize>,
    /// A resumable body's write-through locals: kept across suspensions but
    /// assigned only outside loops, so each assignment also writes the
    /// local's slot and no suspension saves it.
    pub(crate) write_through: BTreeSet<usize>,
    /// The type arguments of the call `pause` is emitting.
    pub(crate) call_types: Vec<String>,
    /// An inline body's structured control flow, built when it is emitted;
    /// a resumable body has none.
    pub(crate) structured: Option<Structured>,
}

type Gen<T> = Result<T, Rejection>;

/// Loop iterations of a resumable body per check for a yield; see
/// `Generator::leaf`.
const YIELD_EVERY: u32 = 16;

/// What a suspension keeps: live locals (local `i` in slot `i`), then live
/// array iterators with the slot of their position.
type Saved = (Vec<usize>, Vec<(usize, usize)>);

fn unsupported<T>(what: impl Into<String>) -> Gen<T> {
    Err(Rejection::Unsupported(what.into()))
}

impl<'db> Generator<'_, 'db> {
    fn repr(&self, local: Local) -> Repr {
        self.analysis.reprs[local.0]
    }

    pub(crate) fn function(&mut self, symbol: &str) -> Gen<String> {
        self.prepare();
        if self.suspends {
            return self.resumable_function(symbol);
        }
        // Only an inline body needs structured control flow; a graph the
        // structurer refuses keeps just this body bytecode.
        let structured = structurize(&self.analysis.cfg)
            .map_err(|error| Rejection::Invalid(error.to_string()))?;
        let arity = self.body.locals.len().min(self.arity());
        let mut out = String::new();
        // The body runs in a closure so every way out, `?` included, passes
        // the wrapper: an error leaving the function records the line it was
        // raised on in this function's frame. `depth` is the frame count with
        // that frame on top; a catch also pops whatever compiled callees left
        // above it.
        let declared = self.declared_at.map_or(0, |span| self.line_of(span));
        let _ = writeln!(
            out,
            "#[inline]\npub fn {symbol}(vm: &mut BexVm, args: &[Value]) -> R<Value> {{\n    \
             let depth = vm.frame_depth();\n    let mut line: u32 = {declared};\n    \
             let result = (|| -> R<Value> {{"
        );
        for (i, repr) in self.analysis.reprs.iter().enumerate() {
            if self.analysis.static_types.contains_key(&i) {
                continue;
            }
            let ty = repr.rust_type();
            if (1..=arity).contains(&i) {
                let arg = coerce(&format!("args[{}]", i - 1), Repr::Value, *repr);
                let _ = writeln!(out, "    let mut l{i}: {ty} = {arg};");
            } else {
                let _ = writeln!(out, "    let mut l{i}: {ty} = {};", repr.default_value());
            }
        }
        for iter in self.analysis.array_iters.keys() {
            let _ = writeln!(out, "    let mut i{iter}: i64 = 0;");
        }
        for block in &self.body.blocks {
            match &block.terminator {
                Some(
                    Terminator::Branch { .. }
                    | Terminator::NarrowBind { .. }
                    | Terminator::ShortCircuit { .. },
                ) => {
                    let _ = writeln!(out, "    let mut c{}: bool = false;", block.id.0);
                }
                Some(Terminator::Switch { .. }) => {
                    let _ = writeln!(out, "    let mut d{}: i64 = 0;", block.id.0);
                }
                _ => {}
            }
        }
        // The cause slot: the context of the error a handler is handling,
        // which the VM's cause chain reads off the frame.
        if let Some(slot) = self.cause_slot {
            let _ = writeln!(
                out,
                "    vm.reserve_slots(depth, {});\n    vm.set_slot(depth, {slot}, Value::NULL);",
                self.slots
            );
        }
        let stmts = structured.stmts.clone();
        self.structured = Some(structured);
        self.out.clear();
        self.stmts(&stmts, 1)?;
        out.push_str(&self.out);
        // Every path ends in an `Exit`; this is unreachable, and keeps the
        // function well-typed if a leaf falls through after structuring.
        out.push_str(
            "    #[allow(unreachable_code)]\n    Ok(Value::NULL)\n    })();\n    \
             if result.is_err() {\n        vm.note_line(depth, line);\n    }\n    result\n}\n",
        );
        Ok(out)
    }

    /// A resumable function: the VM drives it through `Resume`/`Step`. Its
    /// arguments and every local live across a suspension sit in its frame's
    /// stack slots (local `i` in slot `i`, an iterator position after the
    /// locals), so the GC sees them while it is suspended. The blocks run in a
    /// `loop { match bb }`, since a resume enters the middle of the function.
    fn resumable_function(&mut self, symbol: &str) -> Gen<String> {
        let arity = self.body.locals.len().min(self.arity());
        let iters = self.iters();
        let slots = self.slots;
        let point_slot = self.point_slot;
        let declared = self.declared_at.map_or(0, |span| self.line_of(span));
        self.out.clear();
        let _ = writeln!(
            self.out,
            "pub fn {symbol}(vm: &mut BexVm, resume: Resume) -> Result<Step, Box<VmError>> {{\n    \
             let depth = vm.frame_depth();\n    let mut line: u32 = {declared};\n    \
             let result = (|| -> Result<Step, Box<VmError>> {{"
        );
        for (i, repr) in self.analysis.reprs.iter().enumerate() {
            if !self.analysis.static_types.contains_key(&i) {
                let _ = writeln!(
                    self.out,
                    "    let mut l{i}: {} = {};",
                    repr.rust_type(),
                    repr.default_value()
                );
            }
        }
        for iter in &iters {
            let _ = writeln!(self.out, "    let mut i{iter}: i64 = 0;");
        }
        if !self.loop_headers.is_empty() {
            let _ = writeln!(self.out, "    let mut iterations: u32 = 0;");
        }
        for block in &self.body.blocks {
            match &block.terminator {
                Some(
                    Terminator::Branch { .. }
                    | Terminator::NarrowBind { .. }
                    | Terminator::ShortCircuit { .. },
                ) => {
                    let _ = writeln!(self.out, "    let mut c{}: bool = false;", block.id.0);
                }
                Some(Terminator::Switch { .. }) => {
                    let _ = writeln!(self.out, "    let mut d{}: i64 = 0;", block.id.0);
                }
                _ => {}
            }
        }
        // Prologue: start, or read the point this body suspended at, restore
        // what it saved and take the result of its request.
        self.in_loop = false;
        self.line(1, "let mut bb: usize;");
        self.line(1, "match resume {");
        self.line(2, "Resume::Start => {");
        self.line(3, &format!("vm.reserve_slots(depth, {slots});"));
        if let Some(cause) = self.cause_slot {
            self.line(3, &format!("vm.set_slot(depth, {cause}, Value::NULL);"));
        }
        // A write-through local's slot holds its value from its first
        // assignment on; before that, its default, so a resume restoring it
        // where liveness overapproximates (a local a terminator assigns)
        // reads a well-typed value, as when every suspension saved it.
        for local in self.write_through.clone() {
            if local > arity {
                let default = coerce(
                    self.repr(Local(local)).default_value(),
                    self.repr(Local(local)),
                    Repr::Value,
                );
                self.line(3, &format!("vm.set_slot(depth, {local}, {default});"));
            }
        }
        // Argument `i` arrives in slot `i - 1`, but local `i` keeps slot
        // `i`: read every argument before writing any write-through slot,
        // which would land on the next argument.
        for i in 1..=arity {
            let arg = coerce(
                &format!("vm.slot(depth, {})", i - 1),
                Repr::Value,
                self.repr(Local(i)),
            );
            self.line(3, &format!("l{i} = {arg};"));
        }
        let synced: Vec<usize> = (1..=arity)
            .filter(|i| self.write_through.contains(i))
            .collect();
        for i in synced {
            let slot = coerce(&format!("l{i}"), self.repr(Local(i)), Repr::Value);
            self.line(3, &format!("vm.set_slot(depth, {i}, {slot});"));
        }
        let entry = self.body.entry.0;
        self.line(3, &format!("bb = {entry};"));
        self.line(2, "}");
        // Each resume arm restores what its suspension saved (`saved_at`).
        self.line(2, "resume => {");
        self.line(
            3,
            &format!("let point = bex_vm::compiled_rt::expect_int(vm.slot(depth, {point_slot}))?;"),
        );
        self.line(3, "match (point, resume) {");
        for block in 0..self.body.blocks.len() {
            if self.block_suspends(block) {
                self.resume_arms(BlockId(block), 4)?;
            }
        }
        for header in self.loop_headers.clone() {
            self.line(
                4,
                &format!("({}, Resume::Yielded) => {{", self.yield_point(header)),
            );
            let live = self.live_in[header].clone();
            self.restore(self.saved(live), 5);
            self.line(5, &format!("bb = {header};"));
            self.line(4, "}");
        }
        self.line(4, "(_, Resume::Thrown(e)) => return Err(e),");
        self.line(4, "_ => return Err(bex_vm::compiled_rt::unreachable()),");
        self.line(3, "}");
        self.line(2, "}");
        self.line(1, "}");
        // The blocks.
        self.in_loop = true;
        self.line(1, "loop {");
        self.line(2, "match bb {");
        for block in 0..self.body.blocks.len() {
            if self.body.blocks[block].terminator.is_none() {
                continue;
            }
            self.line(3, &format!("{block} => {{"));
            self.leaf(BlockId(block), 4)?;
            self.transfer(BlockId(block), 4)?;
            self.line(3, "}");
        }
        self.line(3, "_ => return Err(bex_vm::compiled_rt::unreachable()),");
        self.line(2, "}");
        self.line(1, "}");
        self.out.push_str(
            "    })();\n    if result.is_err() {\n        vm.note_line(depth, line);\n    }\n    result\n}\n",
        );
        Ok(std::mem::take(&mut self.out))
    }

    /// Work out the body's frame layout and the control-flow facts its
    /// emission reads.
    fn prepare(&mut self) {
        let blocks = &self.body.blocks;
        self.preds = vec![Vec::new(); blocks.len()];
        for block in blocks {
            for successor in block.terminator.iter().flat_map(Terminator::successors) {
                self.preds[successor.0].push(block.id.0);
            }
        }
        let catches = blocks.iter().any(|block| block.handling.is_some());
        if self.suspends {
            let arity = self.body.locals.len().min(self.arity());
            let base = self.analysis.reprs.len().max(arity) + self.iters().len();
            self.point_slot = base;
            self.cause_slot = catches.then_some(base + 1);
            self.slots = base + 1 + usize::from(catches);
            self.live_in = baml_compiler2_mir::live_in_precise(self.body);
            let (headers, in_loop) = loops(self.body);
            self.loop_headers = headers;
            self.write_through = self.write_through_locals(&in_loop);
        } else {
            self.cause_slot = catches.then_some(0);
            self.slots = usize::from(catches);
        }
    }

    /// The locals a resumable body writes through to their slots: each one
    /// that some suspension keeps, is assigned only in blocks outside every
    /// loop (so writing its slot at each assignment runs a bounded number of
    /// times, where saving it runs at every suspension), and is a plain
    /// scalar or value (not a float, which writing would box, nor an
    /// iterator or a handler's landing local, which are assigned elsewhere).
    fn write_through_locals(&self, in_loop: &[bool]) -> BTreeSet<usize> {
        let mut kept = BTreeSet::new();
        for block in 0..self.body.blocks.len() {
            if self.block_suspends(block)
                && let Ok((locals, _)) = self.saved_at(BlockId(block))
            {
                kept.extend(locals);
            }
        }
        for &header in &self.loop_headers {
            kept.extend(self.saved(self.live_in[header].clone()).0);
        }
        let mut excluded: BTreeSet<usize> = self.analysis.array_iters.keys().copied().collect();
        for block in &self.body.blocks {
            if let Some(landing) = block.landing {
                excluded.insert(landing.error_local.0);
                excluded.insert(landing.context_local.0);
            }
        }
        for (block, &looped) in self.body.blocks.iter().zip(in_loop) {
            if !looped {
                continue;
            }
            for statement in &block.statements {
                if let StatementKind::Assign {
                    destination: Place::Local(local),
                    ..
                } = &statement.kind
                {
                    excluded.insert(local.0);
                }
            }
            if let Some(
                Terminator::Call {
                    destination: Place::Local(local),
                    ..
                }
                | Terminator::VirtualCall {
                    destination: Place::Local(local),
                    ..
                }
                | Terminator::SysOp {
                    destination: Place::Local(local),
                    ..
                }
                | Terminator::Await {
                    destination: Place::Local(local),
                    ..
                }
                | Terminator::AwaitAny {
                    destination: Place::Local(local),
                    ..
                }
                | Terminator::Spawn {
                    future: Place::Local(local),
                    ..
                },
            ) = &block.terminator
            {
                excluded.insert(local.0);
            }
            if let Some(Terminator::NarrowBind { destination, .. }) = &block.terminator {
                excluded.insert(destination.0);
            }
        }
        kept.into_iter()
            .filter(|local| {
                !excluded.contains(local) && !matches!(self.repr(Local(*local)), Repr::Float)
            })
            .collect()
    }

    /// What a suspension at the end of `block` keeps in the frame's slots:
    /// each local live once it resumes, then the position of each live array
    /// iterator, with its slot. Live once it resumes means live into the
    /// block the resume continues at, less the local the resume writes, plus
    /// live into the block's handler when a raise can land there. Every other
    /// local is dead across the suspension: it is neither saved nor restored,
    /// and keeps its default until it is next written.
    fn saved_at(&self, block: BlockId) -> Gen<Saved> {
        let (destination, target) = self.resume_edge(block)?;
        let mut live = self.live_in[target.0].clone();
        if let Place::Local(local) = destination {
            live[local.0] = false;
        }
        if let Some(handler) = self.body.blocks[block.0].unwind {
            for (live, handler) in live.iter_mut().zip(&self.live_in[handler.0]) {
                *live |= *handler;
            }
        }
        Ok(self.saved(live))
    }

    /// What a suspension keeps when the locals `live` are live once it
    /// resumes. With a cause slot, every handler's context local is kept too:
    /// a block leaving a nested handler sets the slot back from the enclosing
    /// handler's context (see [`Self::leaf`]), a read MIR's liveness does not
    /// see.
    fn saved(&self, mut live: Vec<bool>) -> Saved {
        if self.cause_slot.is_some() {
            for landing in self.body.blocks.iter().filter_map(|block| block.landing) {
                live[landing.context_local.0] = true;
            }
        }
        let locals = (0..self.analysis.reprs.len())
            .filter(|&i| live[i] && !self.analysis.static_types.contains_key(&i))
            .collect();
        let base = self.analysis.reprs.len().max(self.arity());
        let iters = self
            .iters()
            .into_iter()
            .enumerate()
            .filter(|&(_, iter)| live[iter])
            .map(|(j, iter)| (iter, base + j))
            .collect();
        (locals, iters)
    }

    /// Read back what the suspension at the end of `block` saved.
    fn restore_locals(&mut self, block: BlockId, depth: usize) -> Gen<()> {
        let saved = self.saved_at(block)?;
        self.restore(saved, depth);
        Ok(())
    }

    /// Read `saved` back from the frame's slots.
    fn restore(&mut self, (locals, iters): Saved, depth: usize) {
        for i in locals {
            let value = coerce(
                &format!("vm.slot(depth, {i})"),
                Repr::Value,
                self.repr(Local(i)),
            );
            self.line(depth, &format!("l{i} = {value};"));
        }
        for (iter, slot) in iters {
            self.line(
                depth,
                &format!("i{iter} = bex_vm::compiled_rt::expect_int(vm.slot(depth, {slot}))?;"),
            );
        }
    }

    /// Save `saved` in the frame's slots: all but the write-through locals,
    /// whose slots already hold them.
    fn save(&mut self, (locals, iters): Saved, depth: usize) {
        for i in locals {
            if self.write_through.contains(&i) {
                continue;
            }
            let value = self.value_arg(&format!("l{i}"), self.repr(Local(i)));
            self.statement(depth, &format!("vm.set_slot(depth, {i}, {value});"));
        }
        for (iter, slot) in iters {
            self.statement(
                depth,
                &format!("vm.set_slot(depth, {slot}, Value::int(i{iter}));"),
            );
        }
    }

    /// Suspend at `block`'s terminator: save the locals live across it and
    /// its point in the frame's slots, and hand the VM the request. An await
    /// of a future that has already settled takes its value without
    /// suspending.
    fn pause(&mut self, block: BlockId, depth: usize) -> Gen<()> {
        let point = block.0 + 1;
        let terminator = self.body.blocks[block.0]
            .terminator
            .as_ref()
            .expect("checked");
        // The request, and the arguments a call pushes above the slots.
        let (step, args) = match terminator {
            Terminator::Call {
                callee,
                args,
                ntypeargs,
                argument_layout,
                ..
            } => {
                let (type_args, args) = args.split_at((*ntypeargs).min(args.len()));
                let mut types = Vec::with_capacity(type_args.len());
                for arg in type_args {
                    let ty = match arg {
                        Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
                            self.analysis
                                .static_types
                                .get(&local.0)
                                .map(|ty| ty.code.clone())
                        }
                        _ => None,
                    };
                    let Some(ty) = ty else {
                        return unsupported("call with a type argument known only at runtime");
                    };
                    types.push(ty);
                }
                self.callee_ptr(block, callee)?;
                let args = match callee {
                    Operand::Constant(Constant::Function(func)) => {
                        let name = function_display_name(self.db, *func);
                        self.laid_out_args(block, &name, argument_layout.as_ref(), args)?
                    }
                    _ if argument_layout
                        .as_ref()
                        .is_some_and(|layout| layout.0.iter().any(Option::is_some)) =>
                    {
                        return unsupported("named arguments to a function value");
                    }
                    _ => self.arg_values(block, args)?,
                };
                self.call_types = types;
                (
                    format!("Step::Call {{ callee, argc: {} }}", args.len()),
                    args,
                )
            }
            Terminator::SysOp { callee, args, .. } => {
                self.callee_ptr(block, callee)?;
                let args = self.arg_values(block, args)?;
                (
                    format!("Step::Call {{ callee, argc: {} }}", args.len()),
                    args,
                )
            }
            Terminator::Await {
                future,
                destination,
                target,
                ..
            } => {
                let future = self.load(block, future)?;
                let future = self.value_arg(&future.code, future.repr);
                self.statement(depth, &format!("let operand = {future};"));
                let store = self.store(block, destination, &Expr::new("v", Repr::Value))?;
                self.line(depth, "if let Some(v) = vm.ready(operand) {");
                self.statement(depth + 1, &store);
                let jump = self.jump_to(target.0);
                self.line(depth + 1, &jump);
                self.line(depth, "}");
                ("Step::Await(operand)".to_string(), Vec::new())
            }
            Terminator::AwaitAny { futures, .. } => {
                let futures = self.operand(block, futures)?;
                let futures = self.value_arg(&futures.code, futures.repr);
                self.statement(depth, &format!("let operand = {futures};"));
                ("Step::AwaitAny(operand)".to_string(), Vec::new())
            }
            Terminator::Spawn { plan, .. } => {
                let plan = self.operand(block, plan)?;
                let plan = self.value_arg(&plan.code, plan.repr);
                self.statement(depth, &format!("let operand = {plan};"));
                ("Step::Spawn(operand)".to_string(), Vec::new())
            }
            _ => return Err(Rejection::Invalid("pause at a non-suspending block".into())),
        };
        let saved = self.saved_at(block)?;
        self.save(saved, depth);
        self.flush(depth);
        let point_slot = self.point_slot;
        self.line(
            depth,
            &format!("vm.set_slot(depth, {point_slot}, Value::int({point}));"),
        );
        for arg in &args {
            self.line(depth, &format!("vm.push_arg({arg});"));
        }
        for ty in std::mem::take(&mut self.call_types) {
            self.line(depth, &format!("vm.push_type_arg({ty});"));
        }
        self.line(depth, "vm.note_line(depth, line);");
        self.line(depth, &format!("return Ok({step});"));
        Ok(())
    }

    /// How a block of a resumable function hands control on.
    fn transfer(&mut self, id: BlockId, depth: usize) -> Gen<()> {
        if self.block_suspends(id.0) {
            return self.pause(id, depth);
        }
        let terminator = self.body.blocks[id.0].terminator.as_ref().expect("checked");
        let b = id.0;
        let code = match terminator {
            Terminator::Goto { target }
            | Terminator::Call { target, .. }
            | Terminator::VirtualCall { target, .. } => self.jump_to(target.0),
            Terminator::Branch {
                then_block,
                else_block,
                ..
            }
            | Terminator::NarrowBind {
                then_block,
                else_block,
                ..
            } => format!(
                "bb = if c{b} {{ {} }} else {{ {} }}; continue;",
                then_block.0, else_block.0
            ),
            Terminator::ShortCircuit { eval_rhs, join, .. } => format!(
                "bb = if c{b} {{ {} }} else {{ {} }}; continue;",
                join.0, eval_rhs.0
            ),
            Terminator::Switch {
                arms, otherwise, ..
            } => {
                let mut arms_code = String::new();
                for (key, target) in arms {
                    let SwitchKey::Int(key) = key else {
                        unreachable!("rejected by flow_of")
                    };
                    let _ = write!(arms_code, "{key}i64 => {}, ", target.0);
                }
                format!(
                    "bb = match d{b} {{ {arms_code}_ => {} }}; continue;",
                    otherwise.0
                )
            }
            Terminator::Return
            | Terminator::Unreachable
            | Terminator::Throw { .. }
            | Terminator::Rethrow { .. } => return self.exit(id, depth),
            Terminator::ThrowIfPanic { otherwise, .. } => self.jump_to(otherwise.0),
            Terminator::SysOp { .. }
            | Terminator::Spawn { .. }
            | Terminator::Await { .. }
            | Terminator::AwaitAny { .. } => unreachable!("suspensions are emitted by `pause`"),
        };
        self.line(depth, &code);
        Ok(())
    }

    /// The prologue arms that resume after `block`'s suspension: bind the
    /// result and continue at its target, or land an error in the block's
    /// handler.
    fn resume_arms(&mut self, block: BlockId, depth: usize) -> Gen<()> {
        let point = block.0 + 1;
        let (destination, target) = self.resume_edge(block)?;
        if !matches!(destination, Place::Local(_)) {
            return unsupported("a suspending call's result stored into a field or element");
        }
        let store = self.store(block, &destination, &Expr::new("v", Repr::Value))?;
        self.line(depth, &format!("({point}, Resume::Value(v)) => {{"));
        self.restore_locals(block, depth + 1)?;
        self.line(depth + 1, &store);
        self.line(depth + 1, &format!("bb = {};", target.0));
        self.line(depth, "}");
        if self.body.blocks[block.0].unwind.is_some() {
            self.line(depth, &format!("({point}, Resume::Thrown(e)) => {{"));
            self.restore_locals(block, depth + 1)?;
            let jump = self.unwind_jump(block, "e", depth + 1)?;
            self.flush(depth + 1);
            self.out.push_str(&jump);
            self.line(depth, "}");
        }
        Ok(())
    }

    /// The local a suspension at the end of `block` writes when it resumes,
    /// and the block it continues at.
    fn resume_edge(&self, block: BlockId) -> Gen<(Place, BlockId)> {
        match self.body.blocks[block.0].terminator.as_ref() {
            Some(
                Terminator::Call {
                    destination,
                    target,
                    ..
                }
                | Terminator::SysOp {
                    destination,
                    target,
                    ..
                }
                | Terminator::Await {
                    destination,
                    target,
                    ..
                }
                | Terminator::AwaitAny {
                    destination,
                    target,
                    ..
                },
            ) => Ok((destination.clone(), *target)),
            Some(Terminator::Spawn { future, resume, .. }) => Ok((future.clone(), *resume)),
            _ => Err(Rejection::Invalid(
                "resume after a non-suspending block".into(),
            )),
        }
    }

    /// A jump to `block` in a resumable function.
    fn jump_to(&self, block: usize) -> String {
        if self.in_loop {
            format!("bb = {block}; continue;")
        } else {
            format!("bb = {block};")
        }
    }

    /// The array iterators, in the order their positions take slots after
    /// the locals'.
    fn iters(&self) -> Vec<usize> {
        let mut iters: Vec<usize> = self.analysis.array_iters.keys().copied().collect();
        iters.sort_unstable();
        iters
    }

    /// The point a resumable body records for a yield at loop header
    /// `header`, after the points of its suspending blocks (`block + 1`).
    fn yield_point(&self, header: usize) -> usize {
        self.body.blocks.len() + 1 + header
    }

    /// Whether the handler `block` runs under differs from one it can be
    /// entered from: where the cause slot is set.
    fn cause_changes_at(&self, block: usize) -> bool {
        let handling = self.body.blocks[block].handling;
        let preds = &self.preds[block];
        if preds.is_empty() {
            return handling.is_some();
        }
        preds
            .iter()
            .any(|&pred| self.body.blocks[pred].handling != handling)
    }

    fn line(&mut self, depth: usize, text: &str) {
        for _ in 0..depth {
            self.out.push_str("    ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    /// Emit a statement, preceded by the bindings its expressions hoisted.
    fn statement(&mut self, depth: usize, text: &str) {
        let pending = std::mem::take(&mut self.pending);
        for binding in pending {
            self.line(depth, &binding);
        }
        self.line(depth, text);
    }

    /// Bind `expr` to a temporary unless it is an atom (a local or a
    /// literal), so it can be an argument of a call into the VM.
    fn hoist(&mut self, expr: Expr) -> Expr {
        if is_atom(&expr.code) {
            return expr;
        }
        let name = format!("t{}", self.temps);
        self.temps += 1;
        self.pending.push(format!(
            "let {name}: {} = {};",
            expr.repr.rust_type(),
            expr.code
        ));
        Expr::new(name, expr.repr)
    }

    /// `code`, held as `repr`, coerced to a `Value` that can be a VM call
    /// argument.
    fn value_arg(&mut self, code: &str, repr: Repr) -> String {
        let coerced = coerce(code, repr, Repr::Value);
        self.hoist(Expr::new(coerced, Repr::Value)).code
    }

    /// Emit the bindings hoisted so far.
    fn flush(&mut self, depth: usize) {
        for binding in std::mem::take(&mut self.pending) {
            self.line(depth, &binding);
        }
    }

    /// Emit `line = N;` when the line bytecode attributes to code under
    /// `span` differs from the one the current leaf last set. A terminator is
    /// a sequence point; a statement's faulting instruction follows its
    /// operands, where bytecode takes a span crossing one line break at its
    /// end ([`baml_compiler2_emit::span_line`]).
    fn set_line(&mut self, span: Option<baml_base::Span>, depth: usize, sequence_point: bool) {
        let Some(span) = span else { return };
        let line = baml_compiler2_emit::span_line(
            self.lines,
            span.range.start().into(),
            span.range.end().into(),
            sequence_point,
        );
        if self.current_line != Some(line) {
            self.current_line = Some(line);
            self.line(depth, &format!("line = {line};"));
        }
    }

    /// The line bytecode attributes to a statement starting where `span` does.
    fn line_of(&self, span: baml_base::Span) -> usize {
        baml_compiler2_emit::statement_line(self.lines, span.range.start().into())
    }

    /// Whether `block` ends in a suspension point.
    fn block_suspends(&self, block: usize) -> bool {
        match self.body.blocks[block].terminator.as_ref() {
            Some(
                Terminator::SysOp { .. }
                | Terminator::Spawn { .. }
                | Terminator::Await { .. }
                | Terminator::AwaitAny { .. },
            ) => true,
            Some(terminator @ Terminator::Call { .. }) => self.call_suspends(terminator),
            _ => false,
        }
    }

    /// Whether a `Call` terminator suspends: see [`suspends_directly`]; a
    /// call to a user function suspends unless it is compiled and inline.
    fn call_suspends(&self, terminator: &Terminator<'db>) -> bool {
        if suspends_directly(self.db, self.index, terminator, &self.analysis.static_types) {
            return true;
        }
        let Terminator::Call { callee, .. } = terminator else {
            return false;
        };
        user_callee_name(self.db, self.index, callee)
            .is_some_and(|name| !self.symbols.contains_key(&name) || self.resumable.contains(&name))
    }

    fn arity(&self) -> usize {
        // The MIR body's parameters are locals 1..=arity; `Analysis::new`
        // checked the count against the locals.
        self.analysis.arity
    }
}
