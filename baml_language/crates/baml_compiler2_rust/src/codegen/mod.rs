//! Generates Rust bodies from MIR using the VM runtime and structured control flow.

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

/// Shared program symbols and call properties.
pub(crate) struct ProgramContext<'a, 'db> {
    pub(crate) db: &'db dyn baml_compiler2_mir::Db,
    pub(crate) index: &'a ProgramIndex,
    pub(crate) symbols: &'a HashMap<usize, String>,
    pub(crate) resumable: &'a BTreeSet<usize>,
    /// Calls to these functions require rooting live caller values.
    pub(crate) may_collect: &'a BTreeSet<usize>,
}

pub(crate) struct Generator<'a, 'db> {
    pub(crate) db: &'db dyn baml_compiler2_mir::Db,
    pub(crate) index: &'a ProgramIndex,
    /// Function object whose constant pool and package this body uses.
    pub(crate) object: usize,
    pub(crate) package: Option<u32>,
    pub(crate) symbols: &'a HashMap<usize, String>,
    pub(crate) resumable: &'a BTreeSet<usize>,
    /// Calls to these functions require rooting live caller values.
    pub(crate) may_collect: &'a BTreeSet<usize>,
    pub(crate) gc_checks: crate::admit::GcChecks,
    /// First spill slot used to root values across collecting calls.
    pub(crate) spill_base: usize,
    /// Suspensions require a resumable state machine.
    pub(crate) suspends: bool,
    /// Jumps continue the block loop after the resume prologue.
    pub(crate) in_loop: bool,
    /// Builtin keys mapped to stable binding ordinals.
    pub(crate) builtins: &'a mut BTreeMap<String, usize>,
    pub(crate) body: &'a MirFunctionBody<'db>,
    pub(crate) lambdas: &'a [MirFunction<'db>],
    pub(crate) analysis: &'a Analysis,
    /// Bytecode PCs corresponding to MIR positions.
    pub(crate) positions: &'a Positions,
    /// Source location and shielding last recorded in this leaf.
    pub(crate) current_mark: Option<(usize, bool)>,
    pub(crate) out: String,
    /// Bind non-atomic operands before calls to avoid overlapping VM borrows.
    pub(crate) pending: Vec<String>,
    pub(crate) temps: usize,
    pub(crate) live_in: Vec<Vec<bool>>,
    /// Locals and iterator positions precede the suspension point and cause slots.
    pub(crate) slots: usize,
    /// Active handler context used by the VM cause chain.
    pub(crate) cause_slot: Option<usize>,
    pub(crate) point_slot: usize,
    pub(crate) preds: Vec<Vec<usize>>,
    pub(crate) loop_headers: BTreeSet<usize>,
    /// Locals assigned outside loops whose slots stay current across suspensions.
    pub(crate) write_through: BTreeSet<usize>,
    pub(crate) call_types: Vec<String>,
    /// Inline bodies have structured control flow; resumable bodies do not.
    pub(crate) structured: Option<Structured>,
    /// Frame-derived types hoisted into `ty{k}` locals.
    pub(crate) frame_templates: Vec<baml_compiler2_mir::TyTemplate>,
    /// Type bindings and collections require rebuilding derived types at each use.
    pub(crate) hoist_types: bool,
    /// Static type values cached by their generated expression.
    pub(crate) static_value_sites: Vec<String>,
}

type Gen<T> = Result<T, Rejection>;

impl<'a, 'db> Generator<'a, 'db> {
    pub(crate) fn new(
        program: &ProgramContext<'a, 'db>,
        builtins: &'a mut BTreeMap<String, usize>,
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
            builtins,
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
            self.emit_line(&format!("vm.reserve_slots(depth, {});", self.slots));
        }
        if let Some(slot) = self.cause_slot {
            self.emit_line(&format!("vm.set_slot(depth, {slot}, Value::NULL);"));
        }
        let prologue = std::mem::take(&mut self.out);
        if self.gc_checks.entry {
            self.gc_check(self.body.entry, "vm.gc_due()");
        }
        self.stmts(&stmts)?;
        // Every path ends in an `Exit`; this keeps the closure well-typed.
        self.emit_line("#[allow(unreachable_code)]");
        self.emit_line("Ok(Value::NULL)");
        self.footer();
        Ok(self.assemble(prologue))
    }

    /// Live locals stay in frame slots while the VM suspends this state machine.
    fn resumable_function(&mut self, symbol: &str) -> Gen<String> {
        let arity = self.arity_locals();
        let slots = self.slots;
        let point_slot = self.point_slot;
        self.out.clear();
        self.header(&format!(
            "pub fn {symbol}(vm: &mut BexVm, resume: Resume) -> R<Step>"
        ));
        self.declare_locals(false, !self.loop_headers.is_empty());
        self.in_loop = false;
        self.emit_line("let mut bb: usize;");
        self.emit_line("match resume {");
        self.emit_line("Resume::Start => {");
        self.emit_line(&format!("vm.reserve_slots(depth, {slots});"));
        if let Some(cause) = self.cause_slot {
            self.emit_line(&format!("vm.set_slot(depth, {cause}, Value::NULL);"));
        }
        // Initialize slots so overestimated liveness still restores typed values.
        for local in 0..self.analysis.reprs.len() {
            if !self.write_through.contains(&local) {
                continue;
            }
            if local > arity {
                let repr = self.repr(Local(local));
                let default = coerce(repr.default_value(), repr, Repr::Value);
                self.emit_line(&format!("vm.set_slot(depth, {local}, {default});"));
            }
        }
        // Argument `i` arrives in slot `i - 1` but local `i` keeps slot `i`:
        // read every argument before writing any write-through slot.
        for i in 1..=arity {
            let arg = format!("vm.slot(depth, {})", i - 1);
            if self.body.locals[i].is_captured {
                self.emit_line(&format!("let arg = {arg};"));
                self.emit_line(&format!("l{i} = vm.new_cell(arg);"));
            } else {
                let arg = coerce(&arg, Repr::Value, self.repr(Local(i)));
                self.emit_line(&format!("l{i} = {arg};"));
            }
        }
        let synced: Vec<usize> = (1..=arity)
            .filter(|i| self.write_through.contains(i))
            .collect();
        for i in synced {
            let slot = coerce(&format!("l{i}"), self.repr(Local(i)), Repr::Value);
            self.emit_line(&format!("vm.set_slot(depth, {i}, {slot});"));
        }
        self.emit_line(&format!("bb = {};", self.body.entry.0));
        self.emit_line("}");
        self.emit_line("resume => {");
        self.emit_line(&format!(
            "let point = bex_vm::compiled_rt::expect_int(vm.slot(depth, {point_slot}))?;"
        ));
        self.emit_line("match (point, resume) {");
        for block in 0..self.body.blocks.len() {
            if self.block_suspends(block) {
                self.resume_arms(BlockId(block))?;
            }
        }
        for header in 0..self.body.blocks.len() {
            if !self.loop_headers.contains(&header) {
                continue;
            }
            self.emit_line(&format!(
                "({}, Resume::Yielded) => {{",
                self.yield_point(header)
            ));
            self.restore(self.saved(|i| self.live_in[header][i]));
            self.emit_line(&format!("bb = {header};"));
            self.emit_line("}");
        }
        self.emit_line("(_, Resume::Thrown(e)) => return Err(e),");
        self.emit_line("_ => return Err(bex_vm::compiled_rt::unreachable()),");
        self.emit_line("}");
        self.emit_line("}");
        self.emit_line("}");
        // Every entry rebuilds the frame-derived types, since a collection
        // may have moved their heads while the body was suspended.
        let prologue = std::mem::take(&mut self.out);
        self.in_loop = true;
        self.emit_line("loop {");
        self.emit_line("match bb {");
        for block in 0..self.body.blocks.len() {
            if self.body.blocks[block].terminator.is_none() {
                continue;
            }
            self.emit_line(&format!("{block} => {{"));
            self.leaf(BlockId(block))?;
            self.transfer(BlockId(block))?;
            self.emit_line("}");
        }
        self.emit_line("_ => return Err(bex_vm::compiled_rt::unreachable()),");
        self.emit_line("}");
        self.emit_line("}");
        self.footer();
        Ok(self.assemble(prologue))
    }

    fn arity_locals(&self) -> usize {
        self.body.locals.len().min(self.arity())
    }

    /// Every closure exit records the current PC before returning an error.
    fn header(&mut self, signature: &str) {
        let result = signature
            .rsplit("-> ")
            .next()
            .expect("signature has a result");
        self.emit_line(&format!("{signature} {{"));
        self.emit_line("let depth = vm.frame_depth();");
        self.emit_line("let mut pc: u32 = 0;");
        self.emit_line(&format!("let result = (|| -> {result} {{"));
    }

    fn footer(&mut self) {
        self.emit_line("})();");
        self.emit_line("if result.is_err() {");
        self.emit_line("vm.note_pc(depth, pc);");
        self.emit_line("}");
        self.emit_line("result");
        self.emit_line("}");
    }

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
            self.emit_line(&format!("let mut l{i}: {ty} = {init};"));
        }
        for iter in analysis.array_iters.keys() {
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

    /// Insert hoisted types between the prologue and body.
    fn assemble(&mut self, prologue: String) -> String {
        let body = std::mem::take(&mut self.out);
        for k in 0..self.frame_templates.len() {
            let code = realized_expr(self.index, &self.frame_templates[k], true)
                .expect("a hoisted template was rendered when it was hoisted");
            self.emit_line(&format!("let ty{k}: RealizedTy = {code};"));
        }
        let mut out = prologue;
        out.push_str(&std::mem::take(&mut self.out));
        out.push_str(&body);
        out
    }

    /// Reuse hoisted frame types unless bindings or collections can invalidate them.
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

    /// Read a type from its pooled template with frame arguments substituted.
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

    pub(super) fn realized_all(
        &mut self,
        templates: &[baml_compiler2_mir::TyTemplate],
    ) -> Gen<Vec<String>> {
        templates
            .iter()
            .map(|template| self.realized(template))
            .collect()
    }

    /// Borrow a realized type, binding a temporary when needed.
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

    /// Return realized call types and whether all are static primitives.
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

    /// A resumable body may call a collecting callee before requesting suspension.
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

    fn rooted_locals(&self, block: BlockId) -> Gen<Vec<usize>> {
        let (locals, _) = self.saved_at(block)?;
        Ok(locals
            .into_iter()
            .filter(|&i| self.repr(Local(i)) == Repr::Value)
            .collect())
    }

    /// Root live values across a collecting call and reload them on either outcome.
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

    /// Root live values while collecting and reload them afterward.
    pub(super) fn gc_check(&mut self, block: BlockId, guard: &str) {
        let live = &self.live_in[block.0];
        let locals: Vec<usize> = (0..self.analysis.reprs.len())
            .filter(|&i| {
                live[i] && !self.analysis.is_type_local(i) && self.repr(Local(i)) == Repr::Value
            })
            .collect();
        self.emit_line(&format!("if {guard} {{"));
        let values: Vec<String> = locals.iter().map(|i| format!("l{i}")).collect();
        self.emit_line(&format!("let mut live = [{}];", values.join(", ")));
        self.emit_line("vm.collect_here(&mut live);");
        for (j, i) in locals.iter().enumerate() {
            self.emit_line(&format!("l{i} = live[{j}];"));
        }
        self.emit_line("}");
    }

    /// Write through locals assigned outside loops, excluding boxed floats,
    /// iterators, and handler locals that have separate storage rules.
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
            kept.extend(self.saved(|i| self.live_in[header][i]).0);
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

    /// Keep resume-entry locals except the result, plus unwind-entry locals.
    fn saved_at(&self, block: BlockId) -> Gen<Saved> {
        let (destination, target) = self.resume_edge(block)?;
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
    fn saved(&self, live: impl Fn(usize) -> bool) -> Saved {
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
        let locals = (0..self.analysis.reprs.len())
            .filter(|&i| kept(i) && !self.analysis.is_type_local(i))
            .collect();
        let base = self.analysis.reprs.len().max(self.arity());
        let iters = self
            .iters()
            .into_iter()
            .enumerate()
            .filter(|&(_, iter)| kept(iter))
            .map(|(j, iter)| (iter, base + j))
            .collect();
        (locals, iters)
    }

    fn restore_locals(&mut self, block: BlockId) -> Gen<()> {
        let saved = self.saved_at(block)?;
        self.restore(saved);
        Ok(())
    }

    fn restore(&mut self, (locals, iters): Saved) {
        for i in locals {
            let value = coerce(
                &format!("vm.slot(depth, {i})"),
                Repr::Value,
                self.repr(Local(i)),
            );
            self.emit_line(&format!("l{i} = {value};"));
        }
        for (iter, slot) in iters {
            self.emit_line(&format!(
                "i{iter} = bex_vm::compiled_rt::expect_int(vm.slot(depth, {slot}))?;"
            ));
        }
    }

    /// Write-through locals already have current frame slots.
    fn save(&mut self, (locals, iters): Saved) {
        for i in locals {
            if self.write_through.contains(&i) {
                continue;
            }
            let value = self.value_arg(&format!("l{i}"), self.repr(Local(i)));
            self.emit_statement(&format!("vm.set_slot(depth, {i}, {value});"));
        }
        for (iter, slot) in iters {
            self.emit_statement(&format!("vm.set_slot(depth, {slot}, Value::int(i{iter}));"));
        }
    }

    /// Save live locals and request suspension; settled awaits continue immediately.
    fn pause(&mut self, block: BlockId) -> Gen<()> {
        let point = block.0 + 1;
        let terminator = self.body.blocks[block.0]
            .terminator
            .as_ref()
            .expect("checked");
        let (step, args) = self.pause_request(block, terminator)?;
        let saved = self.saved_at(block)?;
        self.save(saved);
        self.flush_bindings();
        let point_slot = self.point_slot;
        self.emit_line(&format!(
            "vm.set_slot(depth, {point_slot}, Value::int({point}));"
        ));
        for arg in &args {
            self.emit_line(&format!("vm.push_arg({arg});"));
        }
        for ty in std::mem::take(&mut self.call_types) {
            self.emit_line(&format!("vm.push_type_arg({ty});"));
        }
        let site = self.site_pc(block);
        self.emit_line(&format!("vm.note_pc(depth, {site});"));
        self.emit_line(&format!("return Ok({step});"));
        Ok(())
    }

    fn pause_request(
        &mut self,
        block: BlockId,
        terminator: &Terminator<'db>,
    ) -> Gen<(String, Vec<String>)> {
        Ok(match terminator {
            Terminator::Call { .. } => self.call_request(block, terminator)?,

            Terminator::SysOp { callee, args, .. } => {
                self.callee_ptr(block, callee)?;
                let args = self.arg_values(block, args)?;
                (
                    format!("Step::Call {{ callee, argc: {} }}", args.len()),
                    args,
                )
            }
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
                let args = if *has_trace {
                    let Some((trace, args)) = args.split_last() else {
                        return Err(Rejection::Invalid(
                            "a traced call without its attachment".into(),
                        ));
                    };
                    let trace = self.value_operand(block, trace)?;
                    let code = self.fallible(block, &format!("vm.set_call_trace({trace})"))?;
                    self.emit_statement(&format!("{code};"));
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
                self.emit_statement(&format!("let operand = {future};"));
                let store = self.store(block, destination, &Expr::new("v", Repr::Value))?;
                self.emit_line("if let Some(v) = vm.ready(operand) {");
                self.emit_statement(&store);
                let jump = self.jump_to(target.0);
                self.emit_line(&jump);
                self.emit_line("}");
                ("Step::Await(operand)".to_string(), Vec::new())
            }
            Terminator::AwaitAny { futures, .. } => {
                let futures = self.value_operand(block, futures)?;
                self.emit_statement(&format!("let operand = {futures};"));
                ("Step::AwaitAny(operand)".to_string(), Vec::new())
            }
            Terminator::Spawn { plan, .. } => {
                let plan = self.value_operand(block, plan)?;
                self.emit_statement(&format!("let operand = {plan};"));
                ("Step::Spawn(operand)".to_string(), Vec::new())
            }
            _ => return Err(Rejection::Invalid("pause at a non-suspending block".into())),
        })
    }

    fn call_request(
        &mut self,
        block: BlockId,
        terminator: &Terminator<'db>,
    ) -> Gen<(String, Vec<String>)> {
        let Terminator::Call {
            callee,
            args,
            ntypeargs,
            argument_layout,
            has_trace,
            ..
        } = terminator
        else {
            unreachable!("call request requires a call terminator");
        };
        let (type_args, args) = args.split_at((*ntypeargs).min(args.len()));
        // The final operand of a traced call is its attachment.
        let args = if *has_trace {
            let Some((trace, args)) = args.split_last() else {
                return Err(Rejection::Invalid(
                    "a traced call without its attachment".into(),
                ));
            };
            let trace = self.value_operand(block, trace)?;
            let code = self.fallible(block, &format!("vm.set_call_trace({trace})"))?;
            self.emit_statement(&format!("{code};"));
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
                    return unsupported(format!("callee `{name}` not found in the program"));
                };
                self.laid_out_args(block, &name, info, argument_layout.as_ref(), args)?
            }
            // Function values need their named arguments mapped at runtime.
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
                self.emit_statement(&format!(
                    "let argc = vm.push_indirect_args(callee, &[{}], &[{}])?;",
                    slots.join(", "),
                    values.join(", ")
                ));
                laid_out = true;
                Vec::new()
            }
            _ => self.arg_values(block, args)?,
        };
        if !matches!(callee, Operand::Constant(_)) && types.is_empty() && !*has_trace && !laid_out {
            self.direct_closure_call(block, &args)?;
        }
        self.call_types = types;
        Ok(if laid_out {
            ("Step::Call { callee, argc }".to_string(), args)
        } else {
            (
                format!("Step::Call {{ callee, argc: {} }}", args.len()),
                args,
            )
        })
    }

    /// Run compiled inline closures in place before falling back to a VM request.
    fn direct_closure_call(&mut self, block: BlockId, args: &[String]) -> Gen<()> {
        let (destination, target) = self.resume_edge(block)?;
        self.flush_bindings();
        self.emit_line(&format!(
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
        self.emit_statement(&store);
        let jump = self.jump_to(target.0);
        self.emit_line(&jump);
        self.emit_line("}");
        Ok(())
    }

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
        self.emit_line(&code);
        Ok(())
    }

    /// Resume with a result or route an error to the suspension site's handler.
    fn resume_arms(&mut self, block: BlockId) -> Gen<()> {
        let point = block.0 + 1;
        let (destination, target) = self.resume_edge(block)?;
        if !matches!(destination, Place::Local(_) | Place::Deref(_)) {
            return unsupported("a suspending call's result stored into a field or element");
        }
        self.emit_line(&format!("({point}, Resume::Value(v)) => {{"));
        self.restore_locals(block)?;
        let store = self.store(block, &destination, &Expr::new("v", Repr::Value))?;
        self.emit_statement(&store);
        self.emit_line(&format!("bb = {};", target.0));
        self.emit_line("}");
        if self.body.blocks[block.0].unwind.is_some() {
            self.emit_line(&format!("({point}, Resume::Thrown(e)) => {{"));
            self.restore_locals(block)?;
            let jump = self.unwind_jump(block, "e")?;
            self.flush_bindings();
            self.out.push_str(&jump);
            self.emit_line("}");
        }
        Ok(())
    }

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

    fn jump_to(&self, block: usize) -> String {
        if self.in_loop {
            format!("bb = {block}; continue;")
        } else {
            format!("bb = {block};")
        }
    }

    /// Iterator positions take slots in this order after the locals.
    fn iters(&self) -> Vec<usize> {
        self.analysis.array_iters.keys().copied().collect()
    }

    /// Yield points follow suspension points so their IDs cannot overlap.
    fn yield_point(&self, header: usize) -> usize {
        self.body.blocks.len() + 1 + header
    }

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

    fn emit_line(&mut self, text: &str) {
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn emit_statement(&mut self, text: &str) {
        let pending = std::mem::take(&mut self.pending);
        for binding in pending {
            self.emit_line(&binding);
        }
        self.emit_line(text);
    }

    /// VM call arguments must be atoms to avoid overlapping mutable borrows.
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

    fn value_operand(&mut self, block: BlockId, operand: &Operand<'db>) -> Gen<String> {
        let operand = self.operand(block, operand)?;
        Ok(self.value_arg(&operand.code, operand.repr))
    }

    fn value_arg(&mut self, code: &str, repr: Repr) -> String {
        let coerced = coerce(code, repr, Repr::Value);
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
        let mark = self.positions.reading(pc);
        if self.current_mark != Some(mark) {
            self.current_mark = Some(mark);
            self.emit_line(&format!("pc = {pc};"));
        }
    }

    fn site_pc(&self, block: BlockId) -> String {
        pc_arg(self.positions.block(block.0).and_then(|pcs| pcs.terminator))
    }

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

    /// Array iteration is inline; other virtual calls can suspend.
    pub(super) fn array_iteration(&self, terminator: &Terminator<'db>) -> bool {
        array_iter_call(self.body, self.analysis.arity, terminator).is_some()
            || array_next_call(terminator, &self.analysis.array_iters).is_some()
    }

    /// Uncompiled or resumable callees require a VM request.
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
        self.analysis.arity
    }
}
