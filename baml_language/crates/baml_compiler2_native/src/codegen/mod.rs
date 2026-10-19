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
    BlockId, Constant, Local, MirFunction, MirFunctionBody, Operand, Place, StatementKind,
    Terminator, function_display_name,
};

use crate::{
    admit::{Positions, ProgramIndex, Rejection},
    analysis::{
        Analysis, Repr, array_iter_call, array_next_call, loops, reads_frame, realized_expr,
        suspends_directly, template_blocker, user_callee,
    },
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

/// What every body of one program is generated against.
pub(crate) struct ProgramContext<'a, 'db> {
    pub(crate) db: &'db dyn baml_compiler2_mir::Db,
    pub(crate) index: &'a ProgramIndex,
    /// The admitted functions' symbols, by object index.
    pub(crate) symbols: &'a HashMap<usize, String>,
    /// The admitted functions that can suspend, by object index.
    pub(crate) resumable: &'a BTreeSet<usize>,
    /// The admitted inline functions that may collect garbage: a call to one
    /// roots the caller's live values for its duration (`rooted_call`).
    pub(crate) may_collect: &'a BTreeSet<usize>,
}

pub(crate) struct Generator<'a, 'db> {
    pub(crate) db: &'db dyn baml_compiler2_mir::Db,
    pub(crate) index: &'a ProgramIndex,
    /// Object index of the bytecode function this body replaces: the
    /// function whose constant pool and package the body reads.
    pub(crate) object: usize,
    /// The ordinal of the package the function belongs to, when the program
    /// places it.
    pub(crate) package: Option<u32>,
    /// The admitted functions' symbols, by object index.
    pub(crate) symbols: &'a HashMap<usize, String>,
    /// The admitted functions that can suspend, by object index.
    pub(crate) resumable: &'a BTreeSet<usize>,
    /// The admitted inline functions that may collect garbage: a call to one
    /// roots the caller's live values for its duration (`rooted_call`).
    pub(crate) may_collect: &'a BTreeSet<usize>,
    /// Where this body checks for a collection, if it is inline.
    pub(crate) gc_checks: crate::admit::GcChecks,
    /// The first of the frame slots a call that may collect roots the
    /// caller's live values in (`rooted_call`), after the body's own.
    pub(crate) spill_base: usize,
    /// Whether this function can suspend: it is emitted as a resumable state
    /// machine over its blocks rather than as structured code.
    pub(crate) suspends: bool,
    /// While emitting a resumable function's block loop (where a jump is
    /// `bb = N; continue`), as opposed to its resume prologue.
    pub(crate) in_loop: bool,
    /// Builtin native keys the module binds, by key → ordinal.
    pub(crate) natives: &'a mut BTreeMap<String, usize>,
    pub(crate) body: &'a MirFunctionBody<'db>,
    /// The lambdas lowered inside this function, which its `MakeClosure`s
    /// index.
    pub(crate) lambdas: &'a [MirFunction<'db>],
    pub(crate) analysis: &'a Analysis,
    /// Where the body's MIR positions landed in its function's bytecode: the
    /// PCs it records (`pc`).
    pub(crate) positions: &'a Positions,
    /// What the PC the code emitted so far in the current leaf has set reads
    /// as ([`Positions::reading`]).
    pub(crate) current_mark: Option<(usize, bool)>,
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
    /// The templates over the frame's type arguments the body realizes
    /// once on entry, by the local `ty{k}` holding each (`Self::realized`).
    pub(crate) frame_templates: Vec<baml_compiler2_mir::TyTemplate>,
    /// Whether derived types are built once on entry. A body binding a type
    /// into its frame (`type T = ..`) builds each where it is used instead,
    /// since a template may read the slot bound.
    pub(crate) hoist_types: bool,
    /// The statically known types the body reads as `type` values, by the
    /// expression building each: one cached object per site
    /// (`vm.static_type_value`), as bytecode's `LoadType` keeps.
    pub(crate) static_value_sites: Vec<String>,
}

type Gen<T> = Result<T, Rejection>;

impl<'a, 'db> Generator<'a, 'db> {
    pub(crate) fn new(
        program: &ProgramContext<'a, 'db>,
        natives: &'a mut BTreeMap<String, usize>,
        object: usize,
        unit: &'a crate::admit::Unit<'db>,
        gc_checks: crate::admit::GcChecks,
    ) -> Self {
        Self {
            db: program.db,
            index: program.index,
            object,
            package: unit.package,
            symbols: program.symbols,
            resumable: program.resumable,
            may_collect: program.may_collect,
            gc_checks,
            spill_base: 0,
            suspends: program.resumable.contains(&object),
            in_loop: false,
            natives,
            body: crate::admit::body_of(unit.mir),
            lambdas: &unit.mir.lambdas,
            analysis: &unit.analysis,
            positions: program.index.positions(object),
            current_mark: None,
            out: String::new(),
            pending: Vec::new(),
            temps: 0,
            live_in: Vec::new(),
            slots: 0,
            cause_slot: None,
            point_slot: 0,
            preds: Vec::new(),
            loop_headers: BTreeSet::new(),
            write_through: BTreeSet::new(),
            call_types: Vec::new(),
            structured: None,
            frame_templates: Vec::new(),
            hoist_types: true,
            static_value_sites: Vec::new(),
        }
    }
}

/// Loop iterations of a resumable body per check for a yield; see
/// `Generator::leaf`.
const YIELD_EVERY: u32 = 16;

/// What a suspension keeps: live locals (local `i` in slot `i`), then live
/// array iterators with the slot of their position.
type Saved = (Vec<usize>, Vec<(usize, usize)>);

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
        if !self.positions.fit(self.body) {
            return Err(Rejection::Invalid(
                "the function's bytecode was emitted from other MIR".into(),
            ));
        }
        self.prepare();
        if self.suspends {
            self.resumable_function(symbol)
        } else {
            self.inline_function(symbol)
        }
    }

    /// A body that cannot suspend: structured Rust that runs to completion.
    fn inline_function(&mut self, symbol: &str) -> Gen<String> {
        // A graph the structurer refuses keeps the function bytecode.
        let structured = structurize(&self.analysis.cfg)
            .map_err(|error| Rejection::Invalid(error.to_string()))?;
        let stmts = structured.stmts.clone();
        self.structured = Some(structured);
        self.out.clear();
        self.header(&format!(
            "#[inline]\npub fn {symbol}(vm: &mut BexVm, args: &[Value]) -> R<Value>"
        ));
        self.declare_locals(true, self.gc_checks.loops);
        if self.slots > 0 {
            self.line(&format!("vm.reserve_slots(depth, {});", self.slots));
        }
        if let Some(slot) = self.cause_slot {
            self.line(&format!("vm.set_slot(depth, {slot}, Value::NULL);"));
        }
        let prologue = std::mem::take(&mut self.out);
        if self.gc_checks.entry {
            let live = self.live_in[self.body.entry.0].clone();
            self.gc_check(&live, "vm.gc_due()");
        }
        self.stmts(&stmts)?;
        // Every path ends in an `Exit`; this keeps the closure well-typed.
        self.line("#[allow(unreachable_code)]");
        self.line("Ok(Value::NULL)");
        self.footer();
        Ok(self.assemble(prologue))
    }

    /// A resumable function: the VM drives it through `Resume`/`Step`. Its
    /// arguments and every local live across a suspension sit in its frame's
    /// stack slots (local `i` in slot `i`, an iterator position after the
    /// locals), so the GC sees them while it is suspended. The blocks run in a
    /// `loop { match bb }`, since a resume enters the middle of the function.
    fn resumable_function(&mut self, symbol: &str) -> Gen<String> {
        let arity = self.arity_locals();
        let slots = self.slots;
        let point_slot = self.point_slot;
        self.out.clear();
        self.header(&format!(
            "pub fn {symbol}(vm: &mut BexVm, resume: Resume) -> R<Step>"
        ));
        self.declare_locals(false, !self.loop_headers.is_empty());
        // Prologue: start, or read the point this body suspended at, restore
        // what it saved and take the result of its request.
        self.in_loop = false;
        self.line("let mut bb: usize;");
        self.line("match resume {");
        self.line("Resume::Start => {");
        self.line(&format!("vm.reserve_slots(depth, {slots});"));
        if let Some(cause) = self.cause_slot {
            self.line(&format!("vm.set_slot(depth, {cause}, Value::NULL);"));
        }
        // A write-through local's slot holds its value from its first
        // assignment on, and its default before that, so a resume where
        // liveness overapproximates still reads a well-typed value.
        for local in self.write_through.clone() {
            if local > arity {
                let repr = self.repr(Local(local));
                let default = coerce(repr.default_value(), repr, Repr::Value);
                self.line(&format!("vm.set_slot(depth, {local}, {default});"));
            }
        }
        // Argument `i` arrives in slot `i - 1` but local `i` keeps slot `i`:
        // read every argument before writing any write-through slot.
        for i in 1..=arity {
            let arg = format!("vm.slot(depth, {})", i - 1);
            if self.body.locals[i].is_captured {
                self.line(&format!("let arg = {arg};"));
                self.line(&format!("l{i} = vm.new_cell(arg);"));
            } else {
                let arg = coerce(&arg, Repr::Value, self.repr(Local(i)));
                self.line(&format!("l{i} = {arg};"));
            }
        }
        let synced: Vec<usize> = (1..=arity)
            .filter(|i| self.write_through.contains(i))
            .collect();
        for i in synced {
            let slot = coerce(&format!("l{i}"), self.repr(Local(i)), Repr::Value);
            self.line(&format!("vm.set_slot(depth, {i}, {slot});"));
        }
        self.line(&format!("bb = {};", self.body.entry.0));
        self.line("}");
        // Each resume arm restores what its suspension saved (`saved_at`).
        self.line("resume => {");
        self.line(&format!(
            "let point = bex_vm::compiled_rt::expect_int(vm.slot(depth, {point_slot}))?;"
        ));
        self.line("match (point, resume) {");
        for block in 0..self.body.blocks.len() {
            if self.block_suspends(block) {
                self.resume_arms(BlockId(block))?;
            }
        }
        for header in self.loop_headers.clone() {
            self.line(&format!(
                "({}, Resume::Yielded) => {{",
                self.yield_point(header)
            ));
            let live = self.live_in[header].clone();
            self.restore(self.saved(live));
            self.line(&format!("bb = {header};"));
            self.line("}");
        }
        self.line("(_, Resume::Thrown(e)) => return Err(e),");
        self.line("_ => return Err(bex_vm::compiled_rt::unreachable()),");
        self.line("}");
        self.line("}");
        self.line("}");
        // Every entry rebuilds the frame-derived types, since a collection
        // may have moved their heads while the body was suspended.
        let prologue = std::mem::take(&mut self.out);
        self.in_loop = true;
        self.line("loop {");
        self.line("match bb {");
        for block in 0..self.body.blocks.len() {
            if self.body.blocks[block].terminator.is_none() {
                continue;
            }
            self.line(&format!("{block} => {{"));
            self.leaf(BlockId(block))?;
            self.transfer(BlockId(block))?;
            self.line("}");
        }
        self.line("_ => return Err(bex_vm::compiled_rt::unreachable()),");
        self.line("}");
        self.line("}");
        self.footer();
        Ok(self.assemble(prologue))
    }

    /// The number of parameters the body binds as locals.
    fn arity_locals(&self) -> usize {
        self.body.locals.len().min(self.arity())
    }

    /// Open the function and the closure its body runs in. Every way out of
    /// the closure, `?` included, passes [`Self::footer`], which records the
    /// PC an error was raised at in this function's frame. Before its first
    /// statement a body stands at PC 0, as a bytecode frame does.
    fn header(&mut self, signature: &str) {
        let result = signature
            .rsplit("-> ")
            .next()
            .expect("signature has a result");
        self.line(&format!("{signature} {{"));
        self.line("let depth = vm.frame_depth();");
        self.line("let mut pc: u32 = 0;");
        self.line(&format!("let result = (|| -> {result} {{"));
    }

    fn footer(&mut self) {
        self.line("})();");
        self.line("if result.is_err() {");
        self.line("vm.note_pc(depth, pc);");
        self.line("}");
        self.line("result");
        self.line("}");
    }

    /// Declare the body's locals, iterator positions, iteration counter and
    /// per-block condition and discriminant temporaries. An inline body binds
    /// its parameters from `args`; a resumable one reads them on `Start`.
    fn declare_locals(&mut self, from_args: bool, iterations: bool) {
        let analysis = self.analysis;
        let arity = if from_args { self.arity_locals() } else { 0 };
        for (i, &repr) in analysis.reprs.iter().enumerate() {
            if analysis.is_type_local(i) {
                continue;
            }
            let ty = repr.rust_type();
            let init = if !(1..=arity).contains(&i) {
                repr.default_value().to_string()
            } else if self.body.locals[i].is_captured {
                format!("vm.new_cell(args[{}])", i - 1)
            } else {
                coerce(&format!("args[{}]", i - 1), Repr::Value, repr)
            };
            self.line(&format!("let mut l{i}: {ty} = {init};"));
        }
        for iter in analysis.array_iters.keys() {
            self.line(&format!("let mut i{iter}: i64 = 0;"));
        }
        if iterations {
            self.line("let mut iterations: u32 = 0;");
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
            self.line(&declaration);
        }
    }

    /// `prologue`, then the frame-derived types the body needed, then the
    /// body generated since.
    fn assemble(&mut self, prologue: String) -> String {
        let body = std::mem::take(&mut self.out);
        for (k, template) in self.frame_templates.clone().iter().enumerate() {
            let code = realized_expr(self.index, template, true)
                .expect("a hoisted template was rendered when it was hoisted");
            self.line(&format!("let ty{k}: RealizedTy = {code};"));
        }
        let mut out = prologue;
        out.push_str(&std::mem::take(&mut self.out));
        out.push_str(&body);
        out
    }

    /// An expression of type `RealizedTy` for `template`, owned: a static
    /// template is built in place; one reading the frame's type arguments
    /// is a clone of the local built on entry (`frame_templates`), or built
    /// where it is used when the body binds types (`hoist_types`).
    pub(super) fn realized(&mut self, template: &baml_compiler2_mir::TyTemplate) -> Gen<String> {
        if !reads_frame(template) {
            return match realized_expr(self.index, template, false) {
                Some(code) => Ok(code),
                None => self.pooled_type(template),
            };
        }
        let Some(code) = realized_expr(self.index, template, true) else {
            return self.pooled_type(template);
        };
        if !self.hoist_types {
            let name = format!("t{}", self.temps);
            self.temps += 1;
            self.pending
                .push(format!("let {name}: RealizedTy = {code};"));
            return Ok(name);
        }
        let k = match self.frame_templates.iter().position(|t| t == template) {
            Some(k) => k,
            None => {
                self.frame_templates.push(template.clone());
                self.frame_templates.len() - 1
            }
        };
        Ok(format!("ty{k}.clone()"))
    }

    /// A type this backend does not build from its parts (an interface, a
    /// recursive alias, a function type, an opaque type, ...): the constant
    /// bytecode's own `LoadType` of the same template reads, substituted with
    /// the frame's type arguments as that instruction substitutes it, bound
    /// to a temporary.
    fn pooled_type(&mut self, template: &baml_compiler2_mir::TyTemplate) -> Gen<String> {
        let blocked = || {
            unsupported(format!(
                "type template with {}",
                template_blocker(self.index, template)
            ))
        };
        let Some(anchored) = self.index.anchor(template)? else {
            return blocked();
        };
        let Some(index) = self.index.pooled_constant(
            self.object,
            &bex_vm_types::types::ConstValue::Type(anchored),
        ) else {
            return blocked();
        };
        let name = format!("t{}", self.temps);
        self.temps += 1;
        self.pending.push(format!(
            "let {name}: RealizedTy = vm.frame_type_constant(depth, vm.idx_to_ptr(ObjectIndex::from_raw({})), {index})?;",
            self.object
        ));
        Ok(name)
    }

    /// [`Self::realized`] for each of `templates`.
    pub(super) fn realized_all(
        &mut self,
        templates: &[baml_compiler2_mir::TyTemplate],
    ) -> Gen<Vec<String>> {
        templates
            .iter()
            .map(|template| self.realized(template))
            .collect()
    }

    /// An expression of type `&RealizedTy` for `template`: the entry local
    /// borrowed, or a built type bound to a temporary first.
    pub(super) fn realized_ref(
        &mut self,
        template: &baml_compiler2_mir::TyTemplate,
    ) -> Gen<String> {
        let owned = self.realized(template)?;
        if let Some(local) = owned.strip_suffix(".clone()") {
            return Ok(format!("&{local}"));
        }
        let name = format!("t{}", self.temps);
        self.temps += 1;
        self.pending
            .push(format!("let {name}: RealizedTy = {owned};"));
        Ok(format!("&{name}"))
    }

    /// A call's type arguments (the leading `ntypeargs` operands, each a
    /// type local) as owned `RealizedTy` expressions, and whether every one
    /// is a statically known primitive (`StaticTy::primitive`).
    pub(super) fn call_type_args(
        &mut self,
        type_args: &[Operand<'db>],
    ) -> Gen<(Vec<String>, bool)> {
        let mut types = Vec::with_capacity(type_args.len());
        let mut primitive = true;
        for arg in type_args {
            let local = match arg {
                Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => local.0,
                Operand::Constant(_) | Operand::Copy(_) | Operand::Move(_) => {
                    return unsupported("call with a type argument that is not a type local");
                }
            };
            if let Some(ty) = self.analysis.static_types.get(&local) {
                primitive &= ty.primitive;
                types.push(ty.code.clone());
            } else if let Some(template) = self.analysis.frame_types.get(&local).cloned() {
                primitive = false;
                types.push(self.realized(&template)?);
            } else {
                return unsupported("call with a type argument known only at runtime");
            }
        }
        Ok((types, primitive))
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
        self.hoist_types = !blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement.kind,
                    StatementKind::Intrinsic {
                        op: baml_compiler2_mir::IntrinsicOp::BindType(_),
                        ..
                    }
                )
            })
        });
        // A collection moves runtime declarations the hoisted types name;
        // a body that may meet one builds each type where it is used.
        if self.may_collect.contains(&self.object) || (self.suspends && self.calls_in_place()) {
            self.hoist_types = false;
        }
        if self.suspends {
            let arity = self.arity_locals();
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
            if self.may_collect.contains(&self.object) {
                self.live_in = baml_compiler2_mir::live_in_precise(self.body);
            }
            if self.gc_checks.loops {
                self.loop_headers = loops(self.body).0;
            }
        }
        // Frame slots for the values live across a call that may collect: as
        // many as the widest call site keeps.
        self.spill_base = self.slots;
        if self.may_collect.contains(&self.object) || (self.suspends && self.calls_in_place()) {
            let widest = (0..blocks.len())
                .filter(|&b| {
                    matches!(
                        blocks[b].terminator,
                        Some(Terminator::Call { .. } | Terminator::VirtualCall { .. })
                    )
                })
                .filter_map(|b| self.rooted_locals(BlockId(b)).ok())
                .map(|live| live.len())
                .max()
                .unwrap_or(0);
            self.slots += widest;
        }
    }

    /// Whether a resumable body runs a callee in place that may collect: a
    /// compiled function that may, or a closure or implementation it calls
    /// directly before falling back to a request.
    fn calls_in_place(&self) -> bool {
        self.body
            .blocks
            .iter()
            .any(|block| match &block.terminator {
                Some(Terminator::Call {
                    callee: Operand::Constant(Constant::Function(func)),
                    ..
                }) => self
                    .index
                    .function(self.db, *func)
                    .ok()
                    .flatten()
                    .is_some_and(|(object, _)| self.may_collect.contains(&object)),
                Some(Terminator::Call { .. }) => true,
                Some(terminator @ Terminator::VirtualCall { .. }) => {
                    !self.array_iteration(terminator)
                }
                _ => false,
            })
    }

    /// The value locals live across `block`'s call: what a collection inside
    /// the callee may move.
    fn rooted_locals(&self, block: BlockId) -> Gen<Vec<usize>> {
        let (locals, _) = self.saved_at(block)?;
        Ok(locals
            .into_iter()
            .filter(|&i| self.repr(Local(i)) == Repr::Value)
            .collect())
    }

    /// `call`, an expression of type `R<_>` that runs a callee which may
    /// collect, made with the value locals live across `block`'s call rooted
    /// in the frame's spill slots and read back after it (on either outcome:
    /// a handler reads them too, and the callee's frames, which an error
    /// leaves pushed, stand above the slots). The result, through `block`'s
    /// handler.
    pub(super) fn rooted_call(&mut self, block: BlockId, call: &str) -> Gen<String> {
        let live = self.rooted_locals(block)?;
        if live.is_empty() {
            return self.fallible(block, call);
        }
        let k = self.temps;
        self.temps += 1;
        let base = self.spill_base;
        for (j, i) in live.iter().enumerate() {
            self.pending
                .push(format!("vm.set_slot(depth, {}, l{i});", base + j));
        }
        self.pending.push(format!("let r{k} = {call};"));
        for (j, i) in live.iter().enumerate() {
            self.pending
                .push(format!("l{i} = vm.slot(depth, {});", base + j));
        }
        self.fallible(block, &format!("r{k}"))
    }

    /// A compiled GC check: when a collection is due, collect with the value
    /// locals in `live` rooted and read back.
    pub(super) fn gc_check(&mut self, live: &[bool], guard: &str) {
        let locals: Vec<usize> = (0..self.analysis.reprs.len())
            .filter(|&i| {
                live[i] && !self.analysis.is_type_local(i) && self.repr(Local(i)) == Repr::Value
            })
            .collect();
        self.line(&format!("if {guard} {{"));
        let values: Vec<String> = locals.iter().map(|i| format!("l{i}")).collect();
        self.line(&format!("let mut live = [{}];", values.join(", ")));
        self.line("vm.collect_here(&mut live);");
        for (j, i) in locals.iter().enumerate() {
            self.line(&format!("l{i} = live[{j}];"));
        }
        self.line("}");
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
                }
                | StatementKind::FreshCell { local, .. } = &statement.kind
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
            .filter(|&i| live[i] && !self.analysis.is_type_local(i))
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
    fn restore_locals(&mut self, block: BlockId) -> Gen<()> {
        let saved = self.saved_at(block)?;
        self.restore(saved);
        Ok(())
    }

    /// Read `saved` back from the frame's slots.
    fn restore(&mut self, (locals, iters): Saved) {
        for i in locals {
            let value = coerce(
                &format!("vm.slot(depth, {i})"),
                Repr::Value,
                self.repr(Local(i)),
            );
            self.line(&format!("l{i} = {value};"));
        }
        for (iter, slot) in iters {
            self.line(&format!(
                "i{iter} = bex_vm::compiled_rt::expect_int(vm.slot(depth, {slot}))?;"
            ));
        }
    }

    /// Save `saved` in the frame's slots: all but the write-through locals,
    /// whose slots already hold them.
    fn save(&mut self, (locals, iters): Saved) {
        for i in locals {
            if self.write_through.contains(&i) {
                continue;
            }
            let value = self.value_arg(&format!("l{i}"), self.repr(Local(i)));
            self.statement(&format!("vm.set_slot(depth, {i}, {value});"));
        }
        for (iter, slot) in iters {
            self.statement(&format!("vm.set_slot(depth, {slot}, Value::int(i{iter}));"));
        }
    }

    /// Suspend at `block`'s terminator: save the locals live across it and
    /// its point in the frame's slots, and hand the VM the request. An await
    /// of a future that has already settled takes its value without
    /// suspending.
    fn pause(&mut self, block: BlockId) -> Gen<()> {
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
                has_trace,
                ..
            } => {
                let (type_args, args) = args.split_at((*ntypeargs).min(args.len()));
                // A `$trace` attachment is the final operand: handed to the
                // VM for the call it requests, as bytecode's `SetCallTrace`.
                let args = if *has_trace {
                    let Some((trace, args)) = args.split_last() else {
                        return Err(Rejection::Invalid(
                            "a traced call without its attachment".into(),
                        ));
                    };
                    let trace = self.value_operand(block, trace)?;
                    let code = self.fallible(block, &format!("vm.set_call_trace({trace})"))?;
                    self.statement(&format!("{code};"));
                    args
                } else {
                    args
                };
                let (types, _) = self.call_type_args(type_args)?;
                self.callee_ptr(block, callee)?;
                let mut laid_out = false;
                let args = match callee {
                    Operand::Constant(Constant::Function(func)) => {
                        let name = function_display_name(self.db, *func);
                        let Some((_, info)) = self.index.function(self.db, *func)? else {
                            return unsupported(format!(
                                "callee `{name}` not found in the program"
                            ));
                        };
                        self.laid_out_args(block, &name, info, argument_layout.as_ref(), args)?
                    }
                    // Named arguments to a function value: laid over the
                    // callee's slots at run time, as the interpreter's
                    // `CallIndirect` remaps them, and pushed here.
                    _ if argument_layout
                        .as_ref()
                        .is_some_and(|layout| layout.0.iter().any(Option::is_some)) =>
                    {
                        let layout = argument_layout.as_ref().expect("checked");
                        let slots: Vec<String> = layout
                            .0
                            .iter()
                            .map(|slot| match slot {
                                Some(name) => format!("Some({:?})", name.as_str()),
                                None => "None".to_string(),
                            })
                            .collect();
                        let values = self.arg_values(block, args)?;
                        self.statement(&format!(
                            "let argc = vm.push_indirect_args(callee, &[{}], &[{}])?;",
                            slots.join(", "),
                            values.join(", ")
                        ));
                        laid_out = true;
                        Vec::new()
                    }
                    _ => self.arg_values(block, args)?,
                };
                if !matches!(callee, Operand::Constant(_))
                    && types.is_empty()
                    && !*has_trace
                    && !laid_out
                {
                    self.direct_closure_call(block, &args)?;
                }
                self.call_types = types;
                if laid_out {
                    ("Step::Call { callee, argc }".to_string(), args)
                } else {
                    (
                        format!("Step::Call {{ callee, argc: {} }}", args.len()),
                        args,
                    )
                }
            }
            Terminator::SysOp { callee, args, .. } => {
                self.callee_ptr(block, callee)?;
                let args = self.arg_values(block, args)?;
                (
                    format!("Step::Call {{ callee, argc: {} }}", args.len()),
                    args,
                )
            }
            // The callee is resolved and the arguments pushed here, laid over
            // the slots of whichever implementation resolved.
            Terminator::VirtualCall {
                iface,
                method,
                args,
                ntypeargs,
                self_arg,
                argument_layout,
                destination,
                target,
                has_trace,
                ..
            } => {
                // As for a direct call: the attachment is the final operand,
                // handed to the VM, and the call is always requested.
                let args = if *has_trace {
                    let Some((trace, args)) = args.split_last() else {
                        return Err(Rejection::Invalid(
                            "a traced call without its attachment".into(),
                        ));
                    };
                    let trace = self.value_operand(block, trace)?;
                    let code = self.fallible(block, &format!("vm.set_call_trace({trace})"))?;
                    self.statement(&format!("{code};"));
                    args
                } else {
                    args
                };
                let step = self.virtual_request(
                    block,
                    iface,
                    method,
                    args,
                    *has_trace,
                    *ntypeargs,
                    *self_arg,
                    argument_layout.as_ref(),
                    destination,
                    *target,
                )?;
                (step, Vec::new())
            }
            Terminator::Await {
                future,
                destination,
                target,
                ..
            } => {
                let future = self.load(block, future)?;
                let future = self.value_arg(&future.code, future.repr);
                self.statement(&format!("let operand = {future};"));
                let store = self.store(block, destination, &Expr::new("v", Repr::Value))?;
                self.line("if let Some(v) = vm.ready(operand) {");
                self.statement(&store);
                let jump = self.jump_to(target.0);
                self.line(&jump);
                self.line("}");
                ("Step::Await(operand)".to_string(), Vec::new())
            }
            Terminator::AwaitAny { futures, .. } => {
                let futures = self.value_operand(block, futures)?;
                self.statement(&format!("let operand = {futures};"));
                ("Step::AwaitAny(operand)".to_string(), Vec::new())
            }
            Terminator::Spawn { plan, .. } => {
                let plan = self.value_operand(block, plan)?;
                self.statement(&format!("let operand = {plan};"));
                ("Step::Spawn(operand)".to_string(), Vec::new())
            }
            _ => return Err(Rejection::Invalid("pause at a non-suspending block".into())),
        };
        let saved = self.saved_at(block)?;
        self.save(saved);
        self.flush();
        let point_slot = self.point_slot;
        self.line(&format!(
            "vm.set_slot(depth, {point_slot}, Value::int({point}));"
        ));
        for arg in &args {
            self.line(&format!("vm.push_arg({arg});"));
        }
        for ty in std::mem::take(&mut self.call_types) {
            self.line(&format!("vm.push_type_arg({ty});"));
        }
        let site = self.site_pc(block);
        self.line(&format!("vm.note_pc(depth, {site});"));
        self.line(&format!("return Ok({step});"));
        Ok(())
    }

    /// Before a call through a function value suspends (`pause`): when the
    /// value is a closure (or a plain function) whose body is compiled and
    /// inline, call it directly, as a call to a named compiled function is
    /// made, and continue; anything else goes through the VM. `callee` and
    /// `args` are bound.
    fn direct_closure_call(&mut self, block: BlockId, args: &[String]) -> Gen<()> {
        let (destination, target) = self.resume_edge(block)?;
        self.flush();
        self.line(&format!(
            "if let Some(body) = vm.closure_inline_body(callee, {}) {{",
            args.len()
        ));
        let site = self.site_pc(block);
        let call = self.rooted_call(
            block,
            &format!(
                "vm.call_closure_compiled(callee, body, &[{}], {site})",
                args.join(", ")
            ),
        )?;
        let store = self.store(block, &destination, &Expr::new(call, Repr::Value))?;
        self.statement(&store);
        let jump = self.jump_to(target.0);
        self.line(&jump);
        self.line("}");
        Ok(())
    }

    /// How a block of a resumable function hands control on.
    fn transfer(&mut self, id: BlockId) -> Gen<()> {
        if self.block_suspends(id.0) {
            return self.pause(id);
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
                    let pattern = self.switch_pattern(key)?;
                    let _ = write!(arms_code, "{pattern} => {}, ", target.0);
                }
                format!(
                    "bb = match d{b} {{ {arms_code}_ => {} }}; continue;",
                    otherwise.0
                )
            }
            Terminator::Return
            | Terminator::Unreachable
            | Terminator::Throw { .. }
            | Terminator::Rethrow { .. } => return self.exit(id),
            Terminator::ThrowIfPanic { otherwise, .. } => self.jump_to(otherwise.0),
            Terminator::SysOp { .. }
            | Terminator::Spawn { .. }
            | Terminator::Await { .. }
            | Terminator::AwaitAny { .. } => unreachable!("suspensions are emitted by `pause`"),
        };
        self.line(&code);
        Ok(())
    }

    /// The prologue arms that resume after `block`'s suspension: bind the
    /// result and continue at its target, or land an error in the block's
    /// handler.
    fn resume_arms(&mut self, block: BlockId) -> Gen<()> {
        let point = block.0 + 1;
        let (destination, target) = self.resume_edge(block)?;
        if !matches!(destination, Place::Local(_) | Place::Deref(_)) {
            return unsupported("a suspending call's result stored into a field or element");
        }
        self.line(&format!("({point}, Resume::Value(v)) => {{"));
        self.restore_locals(block)?;
        let store = self.store(block, &destination, &Expr::new("v", Repr::Value))?;
        self.statement(&store);
        self.line(&format!("bb = {};", target.0));
        self.line("}");
        if self.body.blocks[block.0].unwind.is_some() {
            self.line(&format!("({point}, Resume::Thrown(e)) => {{"));
            self.restore_locals(block)?;
            let jump = self.unwind_jump(block, "e")?;
            self.flush();
            self.out.push_str(&jump);
            self.line("}");
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
                }
                | Terminator::VirtualCall {
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
        self.analysis.array_iters.keys().copied().collect()
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

    fn line(&mut self, text: &str) {
        self.out.push_str(text);
        self.out.push('\n');
    }

    /// Emit a statement, preceded by the bindings its expressions hoisted.
    fn statement(&mut self, text: &str) {
        let pending = std::mem::take(&mut self.pending);
        for binding in pending {
            self.line(&binding);
        }
        self.line(text);
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
    /// `operand` as a `Value` call argument.
    fn value_operand(&mut self, block: BlockId, operand: &Operand<'db>) -> Gen<String> {
        let operand = self.operand(block, operand)?;
        Ok(self.value_arg(&operand.code, operand.repr))
    }

    fn value_arg(&mut self, code: &str, repr: Repr) -> String {
        let coerced = coerce(code, repr, Repr::Value);
        self.hoist(Expr::new(coerced, Repr::Value)).code
    }

    /// Emit the bindings hoisted so far.
    fn flush(&mut self) {
        for binding in std::mem::take(&mut self.pending) {
            self.line(&binding);
        }
    }

    /// Emit `pc = N;` for a position whose PC is `pc` when it reads
    /// differently ([`Positions::reading`]: its line, its shielding) from
    /// the one the current leaf last set: a frame at either reads the same,
    /// so the body stores no more often than the line changes. A position
    /// the bytecode emitted no instruction for keeps the last one.
    fn set_pc(&mut self, pc: Option<usize>) {
        let Some(pc) = pc else { return };
        let mark = self.positions.reading(pc);
        if self.current_mark != Some(mark) {
            self.current_mark = Some(mark);
            self.line(&format!("pc = {pc};"));
        }
    }

    /// The PC of the call or suspension ending `block`, as generated code:
    /// its instruction's, where a call records its call site.
    fn site_pc(&self, block: BlockId) -> String {
        pc_arg(self.positions.block(block.0).and_then(|pcs| pcs.terminator))
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
            Some(terminator @ Terminator::VirtualCall { .. }) => !self.array_iteration(terminator),
            _ => false,
        }
    }

    /// Whether a `VirtualCall` terminator is one this backend compiles in
    /// place as array iteration (see [`Analysis::array_iters`]); any other
    /// interface method call resolves its callee at run time and suspends.
    pub(super) fn array_iteration(&self, terminator: &Terminator<'db>) -> bool {
        array_iter_call(self.body, self.analysis.arity, terminator).is_some()
            || array_next_call(terminator, &self.analysis.array_iters).is_some()
    }

    /// Whether a `Call` terminator suspends: see [`suspends_directly`]; a
    /// call to a user function suspends unless it is compiled and inline.
    fn call_suspends(&self, terminator: &Terminator<'db>) -> bool {
        if suspends_directly(
            self.db,
            self.index,
            self.body,
            terminator,
            &self.analysis.static_types,
        ) {
            return true;
        }
        let Terminator::Call { callee, .. } = terminator else {
            return false;
        };
        user_callee(self.db, self.index, callee).is_some_and(|object| {
            !self.symbols.contains_key(&object) || self.resumable.contains(&object)
        })
    }

    fn arity(&self) -> usize {
        // The MIR body's parameters are locals 1..=arity; `Analysis::new`
        // checked the count against the locals.
        self.analysis.arity
    }
}
