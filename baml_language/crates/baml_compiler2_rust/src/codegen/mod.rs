//! Generates Rust bodies from MIR using the VM runtime and structured control flow.

mod alloc;
mod call;
mod constant;
mod dispatch;
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

#[derive(Clone)]
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
    /// Bodies in a recursive cycle: a plain call to one checks the stack.
    pub(crate) recursive: &'a BTreeSet<usize>,
    /// Per compiled body: the locality-mask bits some compiled caller can
    /// set. A bit no caller sets compiles its condition to the locked path.
    pub(crate) masks: &'a HashMap<usize, u64>,
}

pub(crate) struct Generator<'a, 'db> {
    pub(crate) db: &'db dyn baml_compiler2_mir::Db,
    pub(crate) index: &'a ProgramIndex<'a>,
    /// Function object whose constant pool and package this body uses.
    pub(crate) object: usize,
    pub(crate) package: Option<u32>,
    pub(crate) symbols: &'a HashMap<usize, String>,
    /// Functions whose bodies run with a VM frame, never a frame record.
    pub(crate) framed: &'a BTreeSet<usize>,
    pub(crate) signatures: &'a HashMap<usize, Signature>,
    pub(crate) recursive_bodies: &'a BTreeSet<usize>,
    /// The locality-mask bits some caller can set ([`ProgramContext::masks`]).
    pub(crate) mask_bits: u64,
    /// Whether this body may suspend: it then polls at loop back edges.
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
    /// Classes whose class word the body binds in its prologue, for direct
    /// field access (`place::direct_load`).
    pub(crate) class_words: BTreeSet<usize>,
    /// Classes whose words only debug builds of the generated code read:
    /// the assertion on a trusted field access (`class_checked!`).
    pub(crate) debug_class_words: BTreeSet<usize>,
    /// Whether the body suspends where its analysis said it would not: a
    /// virtual call whose targets did not all compile. It is then not plain.
    pub(crate) suspends_late: bool,
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
            framed: program.framed,
            signatures: program.signatures,
            recursive_bodies: program.recursive,
            mask_bits: program.masks.get(&object).copied().unwrap_or(0),
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
            slots: 0,
            cause_slot: None,
            cause_changes: Vec::new(),
            call_types: Vec::new(),
            call_trace: None,
            structured: None,
            static_value_sites: Vec::new(),
            class_words: BTreeSet::new(),
            debug_class_words: BTreeSet::new(),
            suspends_late: false,
        }
    }
}

/// Loop iterations between polls for a yield; see `Generator::loop_check`.
const YIELD_EVERY: u32 = 16;

/// A PC argument of generated code: the position's own, or the last one the
/// body set (`pc`) for a position the bytecode emitted nothing for.
fn pc_arg(pc: Option<usize>) -> String {
    pc.map_or_else(|| "*pc".to_string(), |pc| pc.to_string())
}

/// Whether a generated body keeps state in its own frame record, so a call
/// must push one: it names `fr` in some form other than those a plain call's
/// record (its caller's) serves equally, which are PC notes, object lookups
/// and calls. See `BexVm::plain_calls`.
pub(crate) fn keeps_record_state(code: &str) -> bool {
    // `call_inline` is a devirtualized site's path for a receiver whose class
    // was made at run time; see `dispatch`.
    const BEFORE: [&str; 9] = [
        "vm.record_object(",
        "vm.call_record(",
        "vm.call_record_typed(",
        "vm.call_compiled(",
        "vm.call_inline(",
        "vm.plain_unwound(",
        "(vm, ",
        "|vm, ",
        "vm: &mut BexVm, ",
    ];
    const AFTER: [&str; 2] = [".note_pc(", ": &FrameRecord"];
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    code.match_indices("fr").any(|(at, _)| {
        let (before, after) = (&code[..at], &code[at + 2..]);
        let bounded = !before.ends_with(word) && !after.starts_with(word);
        bounded
            && !BEFORE.iter().any(|prefix| before.ends_with(prefix))
            && !AFTER.iter().any(|suffix| after.starts_with(suffix))
    })
}

/// The entry a compiled caller takes when it cannot call `symbol`'s body
/// plain: with a frame record, or through the boxed entry with a VM frame
/// when the VM wants frames. Out of line and cold, so a call site carries
/// only its plain call; `site` is the caller's call-site position.
fn record_entry(symbol: &str, object: usize, signature: &Signature, typed: &str) -> String {
    let args: Vec<String> = (0..signature.params.len())
        .map(|i| format!("arg{i}"))
        .collect();
    let boxed: Vec<String> = args
        .iter()
        .zip(&signature.params)
        .map(|(arg, repr)| coerce(arg, *repr, Repr::Value))
        .collect();
    let passed = args.iter().fold(String::new(), |mut out, arg| {
        let _ = write!(out, ", {arg}");
        out
    });
    let unbox = match signature.ret {
        Repr::Int => "bex_vm::compiled_rt::expect_int(v)",
        Repr::Bool => "bex_vm::compiled_rt::expect_bool(v)",
        Repr::Float => "bex_vm::compiled_rt::expect_f64(v)",
        Repr::Value => "Ok(v)",
    };
    let (count, boxed) = (boxed.len(), boxed.join(", "));
    let ret = signature.ret.rust_type();
    format!(
        "#[cold]\n#[inline(never)]\npub fn {symbol}_r(vm: &mut BexVm, fr: &FrameRecord, site: u32{typed}, mask: u64) -> R<{ret}> {{\n\
         let callee = vm.record_object(fr, ObjectIndex::from_raw({object}));\n\
         if vm.direct_calls() {{\n\
         vm.call_record(fr, callee, site, |vm: &mut BexVm| -> [Value; {count}] {{ [{boxed}] }}, |vm, fr| {symbol}_d(vm, fr{passed}, mask))\n\
         }} else {{\n\
         let values: [Value; {count}] = [{boxed}];\n\
         match vm.call_compiled(fr, callee, {symbol}, &[], &values, site) {{ Ok(v) => {unbox}, Err(e) => Err(e) }}\n\
         }}\n\
         }}\n"
    )
}

/// The name of the constant that says whether `symbol`'s body may run
/// without a frame record; see [`keeps_record_state`].
pub(crate) fn plain_const(symbol: &str) -> String {
    format!("PLAIN_{symbol}")
}

fn unsupported<T>(what: impl Into<String>) -> Gen<T> {
    Err(Rejection::Unsupported(what.into()))
}

impl<'db> Generator<'_, 'db> {
    fn repr(&self, local: Local) -> Repr {
        self.analysis.reprs[local.0]
    }

    /// `needs` (`escape::EscapeFacts::needs`), or
    /// [`baml_compiler2_mir::escape::NEVER`] when it includes a bit no caller sets.
    fn reachable(&self, needs: u64) -> u64 {
        if needs & !self.mask_bits == 0 {
            needs
        } else {
            baml_compiler2_mir::escape::NEVER
        }
    }

    /// `local` where the object is thread-local given `needs`
    /// (`escape::EscapeFacts::needs`), else `locked`: a branch on the body's
    /// locality mask when that depends on what the caller proved.
    pub(super) fn by_locality(&self, needs: u64, local: &str, locked: &str) -> String {
        match self.reachable(needs) {
            0 => local.to_string(),
            baml_compiler2_mir::escape::NEVER => locked.to_string(),
            needs => {
                format!("(if mask & {needs:#x} == {needs:#x} {{ {local} }} else {{ {locked} }})")
            }
        }
    }

    /// The locality mask a call in `block` passes its callee: bit `i` for each
    /// argument the caller proves thread-local, now or given its own mask.
    pub(super) fn callee_mask(&self, block: BlockId) -> String {
        let Some(needs) = self.analysis.escape.call_arg_needs(block) else {
            return "0".to_string();
        };
        let mut constant = 0u64;
        let mut terms = Vec::new();
        for (i, &needs) in needs
            .iter()
            .enumerate()
            .take(baml_compiler2_mir::escape::MASKED_PARAMS)
        {
            match self.reachable(needs) {
                baml_compiler2_mir::escape::NEVER => {}
                0 => constant |= 1 << i,
                needs => terms.push(format!(
                    "(u64::from(mask & {needs:#x} == {needs:#x}) << {i})"
                )),
            }
        }
        if terms.is_empty() {
            return format!("{constant:#x}");
        }
        if constant != 0 {
            terms.insert(0, format!("{constant:#x}"));
        }
        terms.join(" | ")
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
            .map(|(i, repr)| self.boxed_param(i, *repr))
            .collect();
        self.emit_line(&format!(
            "#[inline]\npub fn {symbol}(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> R<Value> {{"
        ));
        let passed = params.iter().fold(String::new(), |mut out, param| {
            let _ = write!(out, ", {param}");
            out
        });
        // The VM proves nothing about a boxed call's arguments: mask 0.
        self.emit_line(&format!("let r = {symbol}_d(vm, fr{passed}, 0);"));
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
        self.out
            .push_str(&record_entry(symbol, self.object, &signature, &typed));
        // The body reports its position through `pc`, which the direct entry
        // records when an error leaves; it is always inlined into that entry,
        // where `pc` is a local. `mask` has bit `i` set when the caller
        // proved argument `i`, and all it reaches, thread-local
        // (`escape::EscapeFacts::needs`).
        let args = (0..signature.params.len()).fold(String::new(), |mut out, i| {
            let _ = write!(out, ", arg{i}");
            out
        });
        self.emit_line(&format!(
            "#[inline]\npub fn {symbol}_d(vm: &mut BexVm, fr: &FrameRecord{typed}, mask: u64) -> R<{ret}> {{"
        ));
        self.emit_line("let mut pc: u32 = 0;");
        self.emit_line(&format!(
            "let result = {symbol}_b(vm, fr, &mut pc{args}, mask);"
        ));
        self.emit_line("if result.is_err() {");
        self.emit_line("fr.note_pc(pc);");
        self.emit_line("}");
        self.emit_line("result");
        self.emit_line("}");
        self.emit_line(&format!(
            "#[inline(always)]\n#[allow(unused_variables, unused_assignments)]\nfn {symbol}_b(vm: &mut BexVm, fr: &FrameRecord, pc: &mut u32{typed}, mask: u64) -> R<{ret}> {{"
        ));
        self.declare_locals(!self.analysis.loop_headers.is_empty());
        // The call path leaves the arguments in a hooked body's leading slots.
        if self.slots > self.slot_base() {
            self.emit_line(&format!("vm.reserve_slots(fr, {})?;", self.slots));
        }
        if let Some(slot) = self.cause_slot {
            self.emit_line(&format!("vm.set_slot(fr, {slot}, Value::NULL);"));
        }
        if self.recursive {
            self.recursion_check();
        }
        let prologue = self.out.len();
        self.stmts(&stmts)?;
        let words = self.class_word_bindings();
        self.out.insert_str(prologue, &words);
        // Every path ends in an `Exit`; this keeps the body well-typed.
        self.emit_line("#[allow(unreachable_code)]");
        self.emit_line(&format!("Ok({})", signature.ret.default_value()));
        self.emit_line("}");
        Ok(std::mem::take(&mut self.out))
    }

    /// The prologue's class-word bindings. A word only the debug assertion
    /// reads is looked up in debug builds of the generated code only.
    fn class_word_bindings(&self) -> String {
        let word = |class: &usize| {
            format!("bex_vm::compiled_rt::class_word(vm.record_object(fr, ObjectIndex::from_raw({class})))")
        };
        let words = self.class_words.iter().map(|class| (class, word(class)));
        let debug_only = self.debug_class_words.difference(&self.class_words);
        let debug = debug_only.map(|class| {
            (class, format!("if cfg!(debug_assertions) {{ {} }} else {{ 0 }}", word(class)))
        });
        words.chain(debug).fold(String::new(), |mut out, (class, value)| {
            let _ = writeln!(out, "let k{class}: u64 = {value};");
            out
        })
    }

    /// The boxed entry's argument `i`: a class-typed parameter whose fields
    /// the body reads in place is checked once here, as it arrives from the
    /// VM or the host (`BexVm::entering_class`).
    fn boxed_param(&self, i: usize, repr: Repr) -> String {
        let arg = format!("args[{i}]");
        let class = (repr == Repr::Value)
            .then(|| self.local_class(baml_compiler2_mir::Local(i + 1)))
            .flatten();
        match class {
            Some(class) => format!(
                "{{ let word = bex_vm::compiled_rt::class_word(vm.record_object(fr, ObjectIndex::from_raw({class}))); vm.entering_class({arg}, word)? }}"
            ),
            None => coerce(&arg, Repr::Value, repr),
        }
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
        let ty = self.realized(template)?;
        Ok(Expr::new(format!("vm.type_value({ty})"), Repr::Value))
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
    }

    /// Poll at entry of a recursive body so deep recursion can yield.
    fn recursion_check(&mut self) {
        self.emit_line("if vm.should_yield_at_call() { vm.suspend(Step::Yield)?; }");
    }

    pub(super) fn suspension(&mut self, block: BlockId) -> Gen<()> {
        let terminator = self.body.blocks[block.0]
            .terminator
            .as_ref()
            .expect("checked");
        let (destination, _) = self.continuation(block)?;
        let request = self.request(block, terminator)?;
        let call = self.fallible(block, &request)?;
        let store = self.store_entering(block, &destination, &Expr::new(call, Repr::Value))?;
        self.emit_statement(&store);
        Ok(())
    }

    /// An `R<Value>` expression that suspends with `step` after pushing `args`
    /// and the pending call types.
    fn suspend_expr(&mut self, block: BlockId, step: &str, args: &[String]) -> String {
        let mut out = String::from("{ ");
        for arg in args {
            let _ = write!(out, "vm.push_arg({arg})?; ");
        }
        for ty in std::mem::take(&mut self.call_types) {
            let _ = write!(out, "vm.push_type_arg({ty}); ");
        }
        let site = self.site_pc(block);
        let _ = write!(out, "fr.note_pc({site}); vm.suspend({step}) }}");
        self.attach_trace(&out)
    }

    /// The request for a call terminator: a direct callee by its object, an
    /// indirect one through the value (in place when it is a compiled
    /// closure), with the arguments laid out as the site recorded.
    fn call_request(
        &mut self,
        block: BlockId,
        callee: &Operand<'db>,
        args: &[Operand<'db>],
        ntypeargs: usize,
        argument_layout: Option<&baml_type::CallLayout>,
        has_trace: bool,
    ) -> Gen<String> {
        let (type_args, args) = args.split_at(ntypeargs.min(args.len()));
        let types = self.type_operands("call", type_args)?;
        let plain = types.is_empty() && !has_trace;
        self.call_types = types;
        let named = argument_layout
            .as_ref()
            .is_some_and(|layout| layout.0.iter().any(Option::is_some));
        if let Operand::Constant(Constant::Function(func)) = callee {
            let args = self.trace_attachment(block, args, has_trace)?;
            self.callee_ptr(block, callee, None)?;
            let name = function_display_name(self.db, *func);
            let Some((_, info)) = self.index.function(self.db, *func) else {
                return unsupported(format!("callee `{name}` not found in the program"));
            };
            let values = self.laid_out_args(block, &name, info, argument_layout, args)?;
            let step = format!("Step::Call {{ callee, argc: {} }}", values.len());
            return Ok(self.suspend_expr(block, &step, &values));
        }
        let (args, trace) = if has_trace {
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
            let layout = argument_layout.expect("checked");
            let slots = call::named_arg_slots(layout);
            self.emit_statement(&format!(
                "let argc = vm.push_indirect_args(callee, &[{}], &[{}])?;",
                slots,
                values.join(", ")
            ));
            return Ok(self.suspend_expr(block, "Step::Call { callee, argc }", &[]));
        }
        // The VM lays the pushed arguments over the callee's slots, as
        // the interpreter's `CallIndirect` does with its recorded
        // layout: a callee with trailing optionals the site omits.
        let suspend = match argument_layout {
            Some(layout) => {
                let slots = call::named_arg_slots(layout);
                let inner = self.suspend_expr(block, "Step::Call { callee, argc }", &[]);
                format!(
                    "{{ let argc = vm.push_indirect_args(callee, &[{slots}], &[{}])?; {inner} }}",
                    values.join(", ")
                )
            }
            None => {
                let step = format!("Step::Call {{ callee, argc: {} }}", values.len());
                self.suspend_expr(block, &step, &values)
            }
        };
        if !plain {
            return Ok(suspend);
        }
        // A compiled closure runs in place.
        let site = self.site_pc(block);
        let values = values.join(", ");
        Ok(format!(
            "match vm.closure_compiled_body(callee, {}) {{ \
        Some(body) => vm.call_closure_compiled(fr, callee, body, &[{values}], {site}), \
        None => {suspend} }}",
            args.len()
        ))
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
            } => self.call_request(
                block,
                callee,
                args,
                *ntypeargs,
                argument_layout.as_ref(),
                *has_trace,
            )?,
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
                "match vm.set_call_trace({trace}) {{ Ok(()) => {call}, Err(e) => {{ let _ = vm.reserve_slots(fr, {}); Err(e) }} }}",
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
            self.emit_line(&format!("*pc = {pc};"));
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
            self.db,
            self.body,
            self.analysis.arity,
            terminator,
            &self.analysis.array_iters,
        )
        .is_some()
            || array_next_call(self.db, terminator, &self.analysis.array_iters).is_some()
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

#[cfg(test)]
mod tests {
    use super::keeps_record_state;

    #[test]
    fn record_state_is_any_use_of_fr_a_callers_record_cannot_serve() {
        for plain in [
            "pub fn f_d(vm: &mut BexVm, fr: &FrameRecord, mask: u64) { fr.note_pc(pc); }",
            "let k: u64 = class_word(vm.record_object(fr, ObjectIndex::from_raw(3)));",
            "vm.call_record(fr, c, 4, |vm: &mut BexVm| -> [Value; 0] { [] }, |vm, fr| g_d(vm, fr, 0))",
            "Err(e) => Err(vm.plain_unwound(fr, c, e))",
            "let frame = 1; let free = frx;",
        ] {
            assert!(!keeps_record_state(plain), "{plain}");
        }
        for framed in [
            "vm.static_type_value(fr, 3)",
            "vm.set_slot(fr, 0, Value::NULL);",
            "vm.catch_at(error, fr, 1, 2)?",
            "vm.capture_cell(fr, 0)",
        ] {
            assert!(keeps_record_state(framed), "{framed}");
        }
    }
}
