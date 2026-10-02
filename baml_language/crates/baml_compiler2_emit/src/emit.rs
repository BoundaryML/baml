//! Pull-model bytecode emission with stackification.
//!
//! This module implements the code generation phase that uses the analysis
//! results to emit optimized bytecode. Virtual locals are inlined at their
//! use sites instead of being stored to stack slots.

use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
};

use baml_base::Span;
use baml_compiler2_hir::loc::DeclRef;
use baml_compiler2_hir_ty::{
    extern_loc::{ClassRef, FunctionRef},
    layout,
};
use baml_compiler2_mir::{
    BasicBlock, BinOp, BlockId, Constant, IndexKind, IntrinsicOp, Local, LogLevel, MirFunctionBody,
    Operand, Place, RealizedTy, RuntimeTy, Rvalue, StatementKind, SwitchKey as MirSwitchKey,
    Terminator, TyTemplate, TypeTest, UnaryOp,
    memory::{self, CellId},
};
use baml_type::DeclName;
use bex_vm_types::{
    BinOp as VmBinOp, Bytecode, CmpOp, ConstValue, Function, FunctionKind, FunctionOrigin,
    GlobalIndex, Instruction, Object, ObjectIndex, ObjectPool, UnaryOp as VmUnaryOp,
    bytecode::{
        ClassInitPlan, DebugLocalScope, FieldCopy, FieldCopySet, InstructionMeta, JumpTableData,
        LineTableEntry, OperandMeta, SwitchKey, SwitchTable,
    },
};

/// Coarse arithmetic-type classification used by [`try_specialize_binary_op`].
///
/// Collapses `RuntimeTy::Int` / `RuntimeTy::Literal(Int(_))` (and similar) into a
/// single tag so specialization works regardless of whether TIR preserved a
/// literal type after constant-folding.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ArithTyClass {
    Int,
    Float,
    Bigint,
}

// ============================================================================
// Switch Strategy Analysis
// ============================================================================

/// Strategy for emitting a switch statement.
#[derive(Debug)]
enum SwitchStrategy {
    /// Use jump table (O(1) lookup) for dense integer ranges.
    JumpTable { min: i64, max: i64 },
    /// Use a switch table + dense jump table for a sparse switch of 4+ arms,
    /// and for every switch with a declaration key: a declaration's tag is
    /// known only to whoever lays it into an image, and every other strategy
    /// bakes key VALUES as constants nothing can relocate. The table states
    /// the keys; the linker or grafter solves it
    /// ([`SwitchDispatch::solved`](bex_vm_types::bytecode::SwitchDispatch::solved)).
    Table,
    /// Use linear if-else chain (O(n) comparisons).
    IfElseChain,
}

// Tunable thresholds for switch emission strategy
const JUMP_TABLE_MIN_ARMS: usize = 4; // Minimum arms to consider jump table
const JUMP_TABLE_MIN_DENSITY: f64 = 0.5; // Minimum density for jump table
const JUMP_TABLE_MAX_SIZE: usize = 256; // Maximum jump table size
const SWITCH_TABLE_MIN_ARMS: usize = 4; // Minimum arms for a switch table

/// Unwrap a `Result` whose error type is `Infallible`.
#[inline]
fn unwrap_infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

/// Analyze a switch's arms to determine the best emission strategy.
///
/// The thresholds are tunable constants that balance code size, memory usage,
/// and runtime performance.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn analyze_switch(arms: &[(SwitchKey, BlockId)]) -> SwitchStrategy {
    // No arms - use if-else (will just jump to otherwise)
    if arms.is_empty() {
        return SwitchStrategy::IfElseChain;
    }
    // A declaration key has no value here.
    if arms
        .iter()
        .any(|(key, _)| matches!(key, SwitchKey::Declaration(_)))
    {
        return SwitchStrategy::Table;
    }
    let values: Vec<i64> = arms.iter().map(|(key, _)| kind_value(*key)).collect();

    // Find min and max values
    let min = values.iter().copied().min().unwrap();
    let max = values.iter().copied().max().unwrap();
    // Safety: max >= min always, and we limit jump tables to 256 entries
    let range = (max - min + 1) as usize;

    // Calculate density (how much of the range is covered)
    // Safety: precision loss acceptable for density calculation
    let density = arms.len() as f64 / range as f64;

    // Use jump table for dense ranges
    if arms.len() >= JUMP_TABLE_MIN_ARMS
        && density >= JUMP_TABLE_MIN_DENSITY
        && range <= JUMP_TABLE_MAX_SIZE
    {
        SwitchStrategy::JumpTable { min, max }
    }
    // Use a switch table for a sparse but large switch. Only reached when
    // density is too low for JumpTable, so it won't interfere with dense enum
    // discriminant switches.
    else if arms.len() >= SWITCH_TABLE_MIN_ARMS {
        SwitchStrategy::Table
    }
    // Default to if-else chain for small switches
    else {
        SwitchStrategy::IfElseChain
    }
}

/// Number of times the emission strategy chosen by [`analyze_switch`] pulls
/// the discriminant operand. Derived from the same `SwitchStrategy` value the
/// emitter dispatches on, so the stack-carry simulation (`stack_carry`) and
/// the emitters cannot disagree about pull counts:
///
/// - `JumpTable` / `Table` pull exactly once;
/// - `IfElseChain` re-loads the discriminant once per emitted comparison —
///   `arms.len()` minus the exhaustive-final elision — including ZERO pulls
///   for its no-comparison forms (no arms; a single exhaustive arm).
///
/// A stack-carried discriminant is only sound at exactly one pull: the carried
/// value is consumed by the first pull, so later pulls would pop unrelated
/// stack slots and a zero-pull form would orphan it (see `stack_carry`'s
/// `Terminator::Switch` arm, the sole consumer).
pub(crate) fn switch_discriminant_pulls(
    arms: &[(MirSwitchKey<'_>, BlockId)],
    exhaustive: bool,
) -> usize {
    // A class arm dispatches through the switch table in every lane (a value
    // for it exists in no lane's analysis), which pulls exactly once.
    let mut kinds = Vec::with_capacity(arms.len());
    for (key, block) in arms {
        match key {
            MirSwitchKey::Int(value) => kinds.push((SwitchKey::Kind(*value), *block)),
            MirSwitchKey::Class(_) => return 1,
        }
    }
    match analyze_switch(&kinds) {
        SwitchStrategy::JumpTable { .. } | SwitchStrategy::Table => 1,
        SwitchStrategy::IfElseChain => arms.len().saturating_sub(usize::from(exhaustive)),
    }
}

/// The value of a kind key. A declaration key never reaches a value-keyed
/// strategy ([`analyze_switch`] routes it to the switch table).
fn kind_value(key: SwitchKey) -> i64 {
    match key {
        SwitchKey::Kind(value) => value,
        SwitchKey::Declaration(_) => unreachable!("a declaration key has no value at emit"),
    }
}

use crate::{
    MirCodegenContext,
    analysis::{AnalysisResult, LocalClassification, StatementRef},
    pull_semantics::{
        self, LocalAssignBehavior, LocalPullAction, LocalStoreBehavior, PullSink, StackEffectSink,
    },
    refs::PackageRefs,
};

// ============================================================================
// Stackification Codegen
// ============================================================================

/// Pending jump table that needs offset patching after all blocks are emitted.
struct PendingJumpTable {
    /// Index of the jump table in `bytecode.jump_tables`.
    table_idx: usize,
    /// Instruction index where the `JumpTable` instruction is.
    jump_table_pc: usize,
    /// Arms with their target blocks (values will be patched to offsets).
    arms: Vec<(i64, PendingJumpTarget)>,
    /// Default target block.
    otherwise: PendingJumpTarget,
    /// The jump table data being built.
    table: JumpTableData,
}

/// Target kind for a pending jump patch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingJumpTarget {
    /// A normal emitted MIR block target.
    Block(BlockId),
    /// Shared trap target for dead-unreachable MIR targets.
    Trap,
}

/// MIR to bytecode compiler with stackification.
struct StackifyCodegen<'db: 'ctx, 'ctx, 'obj, 'w> {
    /// MIR body being compiled.
    body: &'ctx MirFunctionBody<'ctx>,
    /// Arity (parameter count) of the function being compiled.
    arity: usize,
    /// Line index for the MIR's source file.
    line_starts: &'ctx [u32],
    /// The database link names are rendered through at this boundary.
    db: &'ctx dyn baml_compiler2_mir::Db,

    /// The resolution surface every declaration reference goes through; it
    /// outlives the body (`'db: 'ctx`).
    refs: &'obj mut PackageRefs<'w, 'db>,
    /// Read-only snapshot of pooled class field metadata (name + type, in
    /// field order), by declaration.
    /// Field lookups resolve through this map instead of reading the object
    /// pool, so codegen never reads pool contents (parallel emit compiles
    /// against fragment pools that don't contain the pre-existing objects).
    class_fields: &'ctx crate::ClassFieldSnapshot<'db>,
    /// Object pool this function's codegen mints into: the package's code
    /// bucket, or a worker-local fragment of it under parallel emit.
    objects: &'obj mut ObjectPool,
    /// Unit-convention index of `objects[0]`: the code bucket's base for a
    /// serial pass, a fragment's base under parallel emit — so every index
    /// this codegen embeds is in the unit convention either way.
    objects_base: usize,
    /// String objects this function has minted, by content: a string
    /// constant is minted once per function however many sites load it
    /// (`string_object`). Strings compare by value, so sharing is unobservable.
    string_objects: HashMap<String, usize>,

    /// Analysis results (classifications, def-use, etc.).
    analysis: AnalysisResult<'ctx>,

    /// Maps MIR Local -> stack slot index (only for Real locals).
    local_slots: HashMap<Local, usize>,

    /// Number of extra local slots required for this function frame.
    real_local_count: usize,

    /// Maps `BlockId` -> bytecode instruction index (for jump patching).
    block_addresses: HashMap<BlockId, usize>,

    /// Maps `BlockId` -> instruction index just past the block's last
    /// instruction (its exclusive end). Used to compute catch handler-body PC
    /// extents for the BEP-042 cause chain.
    block_end_addresses: HashMap<BlockId, usize>,

    /// Pending jumps that need patching: (`instruction_index`, `target_block`).
    pending_jumps: Vec<(usize, PendingJumpTarget)>,

    /// Pending jump tables that need patching after all blocks are emitted.
    pending_jump_tables: Vec<PendingJumpTable>,

    /// Dead-unreachable MIR blocks for this function.
    dead_unreachable_blocks: HashSet<BlockId>,

    /// Shared trap PC used when pending jumps target dead-unreachable MIR blocks.
    trap_pc: Option<usize>,

    /// Bytecode being generated.
    bytecode: Bytecode,

    /// Current source span for emitted instructions.
    current_debug_span: Option<Span>,
    /// Whether the next emitted instruction should create a sequence point
    /// line-table entry.
    pending_sequence_point: bool,
    /// Per-line discriminator counters for sequence points.
    next_line_discriminator: HashMap<usize, u32>,

    /// The next block in RPO order (for fall-through optimization).
    next_block: Option<BlockId>,

    /// Instruction index where the currently emitted basic block starts.
    current_block_start: usize,

    /// MIR local types, for field-name resolution and instruction
    /// specialization. Kept at the compiler's head: layout facts are read by
    /// declaration, never through an anchored head.
    local_types: HashMap<Local, RuntimeTy>,

    /// Slot index → variable name mapping for debug metadata.
    slot_names: Vec<String>,

    /// Maps MIR lambda index (index into parent `MirFunction.lambdas`) to the
    /// `ObjectIndex` of the compiled lambda `Function` object in `program.objects`.
    /// Populated by Pass 4 when lambda functions are compiled (Phase 3+).
    lambda_object_indices: Vec<usize>,

    /// Names for each lambda (parallel to `lambda_object_indices`).
    /// Used for debug metadata in `MakeClosure` instructions.
    lambda_names: Vec<String>,

    /// Compile-time types for this function's closure captures, indexed by
    /// `Place::Capture`.
    capture_types: Vec<RuntimeTy>,

    /// Set of locals that are captured by child lambdas and need cell wrapping.
    /// Derived from `LocalDecl.is_captured` during `compile()`.
    /// Reads/writes of these locals use `LoadDeref`/`StoreDeref` instead of
    /// `LoadVar`/`StoreVar`.
    captured_locals: HashSet<Local>,

    /// Locals whose cell may be read or written by a spawned thread.
    ///
    /// This is intentionally narrower than `captured_locals`: ordinary closures
    /// also capture cells, but they do not introduce concurrent access by
    /// themselves. Specialized arithmetic is only unsafe when an operand reads
    /// from a cell that can be touched by a spawned closure.
    spawn_captured_locals: HashSet<Local>,

    /// Capture slots whose cell may be read or written by a spawned thread.
    spawn_captured_captures: HashSet<usize>,
}

impl<'db: 'ctx, 'ctx, 'obj, 'w> StackifyCodegen<'db, 'ctx, 'obj, 'w> {
    fn display_string_operand(value: &str) -> String {
        format!("{value:?}")
    }

    /// Create a new stackification codegen instance.
    #[allow(clippy::needless_pass_by_value)] // ctx is destructured into self fields
    fn new(
        body: &'ctx MirFunctionBody<'ctx>,
        arity: usize,
        line_starts: &'ctx [u32],
        ctx: MirCodegenContext<'db, 'ctx, 'obj, 'w>,
        analysis: AnalysisResult<'ctx>,
    ) -> Self {
        // Pre-size the hot output buffers from the MIR's shape. `emit` pushes
        // one instruction + one parallel `meta` entry per bytecode op, and a
        // MIR statement lowers to a few ops, so growing these from empty costs
        // several doubling reallocations (memcpy of the whole buffer) per
        // function — measurable across a project-wide emit. The estimate only
        // sets initial capacity; being off is harmless.
        let stmt_count: usize = body
            .blocks
            .iter()
            .map(|b| b.statements.len() + 1) // +1 for the terminator
            .sum();
        let est_instructions = stmt_count * 3;
        let mut bytecode = Bytecode::new();
        bytecode.instructions.reserve(est_instructions);
        bytecode.meta.reserve(est_instructions);

        Self {
            body,
            arity,
            line_starts,
            db: ctx.db,
            refs: ctx.refs,
            class_fields: ctx.class_fields,
            objects: ctx.objects,
            objects_base: ctx.objects_base,
            string_objects: HashMap::new(),
            analysis,
            local_slots: HashMap::with_capacity(body.locals.len()),
            real_local_count: 0,
            block_addresses: HashMap::with_capacity(body.blocks.len()),
            block_end_addresses: HashMap::with_capacity(body.blocks.len()),
            pending_jumps: Vec::new(),
            pending_jump_tables: Vec::new(),
            dead_unreachable_blocks: HashSet::new(),
            trap_pc: None,
            bytecode,
            current_debug_span: None,
            pending_sequence_point: false,
            next_line_discriminator: HashMap::new(),
            next_block: None,
            current_block_start: 0,
            local_types: HashMap::with_capacity(body.locals.len()),
            slot_names: Vec::new(),
            lambda_object_indices: ctx.lambda_object_indices.to_vec(),
            lambda_names: ctx.lambda_names.to_vec(),
            capture_types: ctx.capture_types.to_vec(),
            captured_locals: HashSet::new(),
            spawn_captured_locals: HashSet::new(),
            spawn_captured_captures: ctx.spawn_capture_indices.clone(),
        }
    }

    /// Append an object to the pool, returning its program-absolute index
    /// (`objects_base` + local position). The ONLY way codegen adds pool
    /// objects: parallel emit relies on every minted index being expressed
    /// relative to the shared watermark.
    fn mint_object(&mut self, object: Object) -> usize {
        let idx = self.objects_base + self.objects.len();
        self.objects.push(object);
        idx
    }

    /// The program-absolute index of the string object holding `value`,
    /// minted on first use in this function and shared by every later load.
    fn string_object(&mut self, value: &str) -> usize {
        if let Some(&idx) = self.string_objects.get(value) {
            return idx;
        }
        let idx = self.mint_object(Object::String(value.into()));
        self.string_objects.insert(value.to_owned(), idx);
        idx
    }

    /// A field's declared name, by the owning class's identity and the field's
    /// index.
    fn lookup_class_field_name<'s, 'a>(
        &'s self,
        class: ClassRef<'a>,
        field_idx: usize,
    ) -> Option<String>
    where
        'db: 'a,
        'a: 's,
    {
        self.class_fields_for(class)?
            .get(field_idx)
            .map(|(name, _)| name.clone())
    }

    /// A MIR template as the program spells it: what operand metadata
    /// prints. Display only — nothing resolves a declaration from it.
    fn spelled_template(&self, template: &TyTemplate) -> String {
        let spelling = baml_compiler2_hir::package::spelling(self.db);
        template
            .map_heads(&mut |decl: &DeclName| spelling.wire(decl))
            .to_string()
    }

    /// A declaration's display name as the program spells it: what operand
    /// metadata prints for a class or enum head.
    fn spelled_head(&self, head: &DeclName) -> String {
        baml_compiler2_hir::package::spelling(self.db)
            .wire(head)
            .display_name()
            .to_string()
    }

    /// A MIR switch key as this lane's bytecode states it.
    fn resolve_switch_key(&mut self, key: MirSwitchKey<'ctx>) -> SwitchKey {
        match key {
            MirSwitchKey::Int(value) => SwitchKey::Kind(value),
            MirSwitchKey::Class(class) => self.refs.switch_key(class),
        }
    }

    /// A class's field layout from the read-only snapshot, by declaration.
    fn class_fields_for<'s, 'a>(&'s self, class: ClassRef<'a>) -> Option<&'s [(String, RuntimeTy)]>
    where
        'db: 'a,
        'a: 's,
    {
        // The snapshot outlives the body; read it at the body's lifetime.
        let fields: &'s HashMap<ClassRef<'a>, Vec<(String, RuntimeTy)>> = self.class_fields;
        fields.get(&class).map(Vec::as_slice)
    }

    /// Resolve the type of a MIR Place by walking from the root local through projections.
    fn resolve_place_type(&self, place: &Place) -> Option<RuntimeTy> {
        match place {
            // A captured local's declared type is its value's type, so a bare
            // `Local` reports the value type while naming a pointer; nothing
            // asks for a bare pointer's type.
            Place::Local(local) => self.local_types.get(local).cloned(),
            Place::Capture(idx) => self.capture_types.get(*idx).cloned(),
            Place::Deref(CellId::Local(local)) => self.local_types.get(local).cloned(),
            Place::Deref(CellId::Capture(idx)) => self.capture_types.get(*idx).cloned(),
            Place::Field { base, field } => {
                let base_ty = self.resolve_place_type(base)?;
                match &base_ty {
                    RuntimeTy::Class(tn, _) => self
                        .class_fields_for(self.refs.class_ref(tn)?)?
                        .get(*field)
                        .map(|(_, field_type)| field_type.clone()),
                    _ => None,
                }
            }
            Place::Index { base, .. } => {
                let base_ty = self.resolve_place_type(base)?;
                match base_ty {
                    RuntimeTy::List(inner) => Some(*inner),
                    RuntimeTy::Map { value, .. } => Some(*value),
                    _ => None,
                }
            }
        }
    }

    /// Resolve the compile-time type of an operand, if known.
    fn resolve_operand_type(&self, operand: &Operand<'ctx>) -> Option<RuntimeTy> {
        match operand {
            Operand::Constant(c) => match c {
                Constant::Int(_) => Some(RuntimeTy::int()),
                Constant::Bigint(_) => Some(RuntimeTy::bigint()),
                Constant::Float(_) => Some(RuntimeTy::float()),
                Constant::String(_) => Some(RuntimeTy::string()),
                Constant::Bool(_) => Some(RuntimeTy::bool()),
                Constant::Null => Some(RuntimeTy::null()),
                Constant::OmittedArg => None,
                _ => None,
            },
            Operand::Copy(place) | Operand::Move(place) => self.resolve_place_type(place),
        }
    }

    /// Classify a type for binary-op specialization. Returns `None` if the
    /// type isn't one of the primitive numeric forms we can specialize on.
    ///
    /// Both `RuntimeTy::Int` and `RuntimeTy::Literal(Literal::Int(_), _)` map to
    /// `Int`, and similarly for `Float`/`Bigint`. This lets us specialize
    /// expressions like `(-1n) & 255n` where the lhs operand carries a
    /// `RuntimeTy::Literal(Bigint(-1))` after constant-folding in TIR.
    fn classify_arith_ty(ty: &RuntimeTy) -> Option<ArithTyClass> {
        use RuntimeTy as T;
        match ty {
            T::Int => Some(ArithTyClass::Int),
            T::Float => Some(ArithTyClass::Float),
            T::Bigint => Some(ArithTyClass::Bigint),
            T::Literal(baml_type::Literal::Int(_), _) => Some(ArithTyClass::Int),
            T::Literal(baml_type::Literal::Float(_), _) => Some(ArithTyClass::Float),
            T::Literal(baml_type::Literal::Bigint(_), _) => Some(ArithTyClass::Bigint),
            _ => None,
        }
    }

    fn local_reads_spawn_captured_local(&self, local: Local, seen: &mut HashSet<Local>) -> bool {
        if self.spawn_captured_locals.contains(&local) {
            return true;
        }
        if !seen.insert(local) {
            return false;
        }

        match self.analysis.classifications.get(&local).copied() {
            Some(LocalClassification::CopyOf) => {
                let source = self.analysis.resolve_copy_source(local);
                self.local_reads_spawn_captured_local(source, seen)
            }
            Some(LocalClassification::Virtual) => self
                .analysis
                .def_use
                .get(&local)
                .and_then(|du| du.def.as_ref())
                .is_some_and(|def| self.rvalue_reads_spawn_captured_local(&def.rvalue, seen)),
            _ => false,
        }
    }

    fn place_reads_spawn_captured_local(&self, place: &Place, seen: &mut HashSet<Local>) -> bool {
        match place {
            Place::Local(local) => self.local_reads_spawn_captured_local(*local, seen),
            // A bare capture is a pointer, never an arithmetic operand.
            Place::Capture(_) => false,
            Place::Deref(CellId::Local(local)) => self.spawn_captured_locals.contains(local),
            Place::Deref(CellId::Capture(idx)) => self.spawn_captured_captures.contains(idx),
            Place::Field { base, .. } => self.place_reads_spawn_captured_local(base, seen),
            Place::Index { base, index, .. } => {
                self.place_reads_spawn_captured_local(base, seen)
                    || self.local_reads_spawn_captured_local(*index, seen)
            }
        }
    }

    fn operand_reads_spawn_captured_local(
        &self,
        operand: &Operand<'ctx>,
        seen: &mut HashSet<Local>,
    ) -> bool {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => {
                self.place_reads_spawn_captured_local(place, seen)
            }
            Operand::Constant(_) => false,
        }
    }

    fn rvalue_reads_spawn_captured_local(
        &self,
        rvalue: &Rvalue<'ctx>,
        seen: &mut HashSet<Local>,
    ) -> bool {
        match rvalue {
            Rvalue::Use(operand) | Rvalue::UnaryOp { operand, .. } => {
                self.operand_reads_spawn_captured_local(operand, seen)
            }
            Rvalue::BinaryOp { left, right, .. } => {
                self.operand_reads_spawn_captured_local(left, seen)
                    || self.operand_reads_spawn_captured_local(right, seen)
            }
            Rvalue::Array(_, elements)
            | Rvalue::Aggregate {
                fields: elements, ..
            } => elements
                .iter()
                .any(|operand| self.operand_reads_spawn_captured_local(operand, seen)),
            Rvalue::MakeVirtualFunction { type_args, .. } => type_args
                .iter()
                .any(|arg| self.operand_reads_spawn_captured_local(arg, seen)),
            Rvalue::Uint8Array(_)
            | Rvalue::LoadType(_)
            | Rvalue::CurrentPackage(_)
            | Rvalue::MakeGenericFunction { .. } => false,
            Rvalue::MakeGenericFunctionFromValue { value, .. } => {
                self.operand_reads_spawn_captured_local(value, seen)
            }
            Rvalue::Map(_, _, entries) => entries.iter().any(|(key, value)| {
                self.operand_reads_spawn_captured_local(key, seen)
                    || self.operand_reads_spawn_captured_local(value, seen)
            }),
            Rvalue::Discriminant(place) | Rvalue::TypeTag(place) | Rvalue::Len(place) => {
                self.place_reads_spawn_captured_local(place, seen)
            }
            Rvalue::IsType { operand, .. }
            | Rvalue::IsTypeTag { operand, .. }
            | Rvalue::MakeBoundMethod {
                receiver: operand, ..
            }
            | Rvalue::MakeVirtualBoundMethod {
                receiver: operand, ..
            }
            | Rvalue::VirtualFieldAccess {
                receiver: operand, ..
            } => self.operand_reads_spawn_captured_local(operand, seen),
            Rvalue::MakeClosure { captures, .. } => captures
                .iter()
                .any(|operand| self.operand_reads_spawn_captured_local(operand, seen)),
        }
    }

    fn binary_operands_can_use_specialized_op(
        &self,
        left: &Operand<'ctx>,
        right: &Operand<'ctx>,
    ) -> bool {
        let mut seen = HashSet::new();
        !self.operand_reads_spawn_captured_local(left, &mut seen)
            && !self.operand_reads_spawn_captured_local(right, &mut seen)
    }

    /// Try to emit a specialized instruction for a binary operation based on
    /// static operand types. Returns `None` when types can't be resolved or
    /// don't match a specialized form (mixed int/float, strings, bitwise, etc.).
    fn try_specialize_binary_op(
        &self,
        op: BinOp,
        left: &Operand<'ctx>,
        right: &Operand<'ctx>,
    ) -> Option<Instruction> {
        if !self.binary_operands_can_use_specialized_op(left, right) {
            return None;
        }

        let left_ty = self.resolve_operand_type(left)?;
        let right_ty = self.resolve_operand_type(right)?;

        let left_class = Self::classify_arith_ty(&left_ty)?;
        let right_class = Self::classify_arith_ty(&right_ty)?;

        match (left_class, right_class) {
            (ArithTyClass::Int, ArithTyClass::Int) => match op {
                BinOp::Add => Some(Instruction::AddInt),
                BinOp::Sub => Some(Instruction::SubInt),
                BinOp::Mul => Some(Instruction::MulInt),
                BinOp::Div => Some(Instruction::DivInt),
                BinOp::Mod => Some(Instruction::ModInt),
                BinOp::Eq => Some(Instruction::CmpIntOp(CmpOp::Eq)),
                BinOp::Ne => Some(Instruction::CmpIntOp(CmpOp::NotEq)),
                BinOp::Lt => Some(Instruction::CmpIntOp(CmpOp::Lt)),
                BinOp::Le => Some(Instruction::CmpIntOp(CmpOp::LtEq)),
                BinOp::Gt => Some(Instruction::CmpIntOp(CmpOp::Gt)),
                BinOp::Ge => Some(Instruction::CmpIntOp(CmpOp::GtEq)),
                _ => None, // bitwise ops stay generic
            },
            (ArithTyClass::Float, ArithTyClass::Float) => match op {
                BinOp::Add => Some(Instruction::AddFloat),
                BinOp::Sub => Some(Instruction::SubFloat),
                BinOp::Mul => Some(Instruction::MulFloat),
                BinOp::Div => Some(Instruction::DivFloat),
                BinOp::Eq => Some(Instruction::CmpFloatOp(CmpOp::Eq)),
                BinOp::Ne => Some(Instruction::CmpFloatOp(CmpOp::NotEq)),
                BinOp::Lt => Some(Instruction::CmpFloatOp(CmpOp::Lt)),
                BinOp::Le => Some(Instruction::CmpFloatOp(CmpOp::LtEq)),
                BinOp::Gt => Some(Instruction::CmpFloatOp(CmpOp::Gt)),
                BinOp::Ge => Some(Instruction::CmpFloatOp(CmpOp::GtEq)),
                _ => None,
            },
            // A mixed `bigint`/`int` pair routes to the same specialized
            // opcodes: the VM resolves the lone `int` operand to a small local
            // `BigInt` without allocating a heap bigint for it.
            (ArithTyClass::Bigint | ArithTyClass::Int, ArithTyClass::Bigint)
            | (ArithTyClass::Bigint, ArithTyClass::Int) => match op {
                BinOp::Add => Some(Instruction::AddBigint),
                BinOp::Sub => Some(Instruction::SubBigint),
                BinOp::Mul => Some(Instruction::MulBigint),
                BinOp::Div => Some(Instruction::DivBigint),
                BinOp::Mod => Some(Instruction::ModBigint),
                BinOp::BitAnd => Some(Instruction::BitAndBigint),
                BinOp::BitOr => Some(Instruction::BitOrBigint),
                BinOp::BitXor => Some(Instruction::BitXorBigint),
                BinOp::Shl => Some(Instruction::ShlBigint),
                BinOp::Shr => Some(Instruction::ShrBigint),
                BinOp::Eq => Some(Instruction::CmpBigintOp(CmpOp::Eq)),
                BinOp::Ne => Some(Instruction::CmpBigintOp(CmpOp::NotEq)),
                BinOp::Lt => Some(Instruction::CmpBigintOp(CmpOp::Lt)),
                BinOp::Le => Some(Instruction::CmpBigintOp(CmpOp::LtEq)),
                BinOp::Gt => Some(Instruction::CmpBigintOp(CmpOp::Gt)),
                BinOp::Ge => Some(Instruction::CmpBigintOp(CmpOp::GtEq)),
            },
            _ => None,
        }
    }

    fn span_for_statement_ref(&self, block: BlockId, statement_ref: StatementRef) -> Option<Span> {
        let block = self.body.block(block);
        match statement_ref {
            StatementRef::Statement(index) => block.statements.get(index).and_then(|s| s.span),
            StatementRef::Terminator => block.terminator_span,
        }
    }

    fn def_span_for_local(&self, local: Local) -> Option<Span> {
        self.analysis
            .def_use
            .get(&local)
            .and_then(|du| du.def.as_ref())
            .and_then(|def| self.span_for_statement_ref(def.block, def.statement_ref))
    }

    /// Compile a MIR function to bytecode.
    fn compile(mut self) -> Function {
        let mir = self.body;
        // 1. Allocate stack slots only for real locals
        self.allocate_real_locals(mir);

        // Collect captured locals for LoadDeref/StoreDeref emission.
        self.captured_locals = mir
            .locals
            .iter()
            .enumerate()
            .filter(|(_, decl)| decl.is_captured)
            .filter_map(|(i, _)| {
                let local = Local(i);
                self.local_slots.contains_key(&local).then_some(local)
            })
            .collect();
        for cell in memory::spawn_shared_cells(mir) {
            match cell {
                CellId::Local(local) => {
                    self.spawn_captured_locals.insert(local);
                }
                CellId::Capture(idx) => {
                    self.spawn_captured_captures.insert(idx);
                }
            }
        }

        // Build local type map for field name resolution (debug info).
        for (i, local_decl) in mir.locals.iter().enumerate() {
            self.local_types.insert(Local(i), local_decl.ty.clone());
        }

        // Build slot name mapping for debug metadata.
        self.slot_names = Self::build_local_names(mir, &self.local_slots);

        // Wrap each captured parameter's value in a cell at entry. A parameter
        // is the one binding created without a `FreshCell` — the caller wrote
        // its value into the slot — so the frame preamble is where it gets its
        // cell. Every other captured local's cell comes from its `FreshCell`.
        // Emitted after the slot names exist so the instructions carry them.
        for local in (1..=self.arity).map(Local) {
            if !mir.local(local).is_captured {
                continue;
            }
            let Some(&slot) = self.local_slots.get(&local) else {
                unreachable!("a captured parameter is always Real");
            };
            let inst = self.emit(Instruction::LoadVar(slot));
            self.set_var_operand(inst, slot);
            self.emit(Instruction::MakeCell);
            let inst = self.emit(Instruction::StoreVar(slot));
            self.set_var_operand(inst, slot);
        }

        // 2. Emit blocks in RPO order.
        //
        // We skip:
        // - dead unreachable blocks, and
        // - non-entry redirect-source blocks (threaded through by analysis).
        //
        // Redirect-source blocks are effectively empty at bytecode level and keeping
        // them would emit dead jumps. We intentionally do not assign those blocks
        // bytecode addresses so unresolved references fail loudly during patching.
        let rpo = self.analysis.rpo.clone();
        let is_dead_unreachable: Vec<bool> = rpo
            .iter()
            .map(|&block_id| crate::analysis::is_dead_unreachable_block(mir.block(block_id)))
            .collect();
        self.dead_unreachable_blocks = rpo
            .iter()
            .enumerate()
            .filter_map(|(i, &block_id)| is_dead_unreachable[i].then_some(block_id))
            .collect();
        let should_emit: Vec<bool> = rpo
            .iter()
            .enumerate()
            .map(|(i, &block_id)| {
                !is_dead_unreachable[i]
                    && (block_id == mir.entry
                        || !self.analysis.redirect_targets.contains_key(&block_id))
            })
            .collect();

        let mut next_emitted_after: Vec<Option<BlockId>> = vec![None; rpo.len()];
        let mut next_emitted = None;
        for i in (0..rpo.len()).rev() {
            next_emitted_after[i] = next_emitted;
            if should_emit[i] {
                next_emitted = Some(rpo[i]);
            }
        }

        for (i, &block_id) in rpo.iter().enumerate() {
            // Track the next *emitted* block for fall-through optimization.
            self.next_block = next_emitted_after[i];

            if is_dead_unreachable[i] {
                continue;
            }

            if !should_emit[i] {
                continue;
            }

            let block_start = self.current_pc();
            self.block_addresses.insert(block_id, block_start);
            self.current_block_start = block_start;
            let block = mir.block(block_id);
            self.emit_block(block);
            self.block_end_addresses.insert(block_id, self.current_pc());
        }

        // If any pending edges target dead-unreachable MIR blocks, patch them
        // through a shared trap target instead of assigning fake block addresses.
        self.ensure_trap_pc_if_needed();

        // 3. Patch all jump targets and jump tables
        self.patch_jumps();
        self.patch_jump_tables();

        // 4. Build exception table from the blocks' handler edges
        self.build_exception_table(mir);
        self.build_shield_table(mir);

        let debug_locals = Self::build_debug_locals(mir, &self.local_slots);

        // 5. Build the Function
        // Note: `name` is set by the caller after `compile_mir_function` returns.
        // `span` is set by `compile_mir_function` from the MIR function span.
        Function {
            name: String::new(),
            source_file: String::new(), // caller sets this after compile_mir_function returns
            docstring: None,
            declared_name: None,
            arity: self.arity,
            real_local_count: self.real_local_count,
            bytecode: self.bytecode,
            kind: FunctionKind::Bytecode,
            telemetry_function_id: None,
            telemetry_registration: bex_vm_types::FunctionRegistration::default(),
            telemetry_policy_id: bex_vm_types::TelemetryPolicyId::none(),
            local_names: self.slot_names,
            debug_locals,
            span: Span::fake(),
            return_type: bex_vm_types::TyTemplate::Null,
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            type_param_names: Vec::new(),
            generic_param_bounds: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type: "null".to_string(),
            throws_type: bex_vm_types::TyTemplate::Never,
            origin: FunctionOrigin::Internal,
            is_interface_body: false, // set from the item tree by attach_function_metadata
            native_key: None,
            body_meta: None,

            runtime_package: bex_vm_types::HeapPtr::null(),
        }
    }

    /// Allocate stack slots only for Real locals.
    ///
    /// Virtual locals don't get slots - they're inlined at use sites.
    fn allocate_real_locals(&mut self, mir: &MirFunctionBody<'ctx>) {
        self.local_slots.clear();
        self.real_local_count = 0;
        let arity = self.arity;

        // Count how many real locals we need to pre-allocate
        let mut next_slot = arity + 1; // Start after params (slot 0 is fn ref, 1..=arity are params)
        let mut slots_to_allocate = 0;

        for (idx, _) in mir.locals.iter().enumerate() {
            let local = Local(idx);
            let classification = self.analysis.classifications[&local];

            match classification {
                LocalClassification::Parameter => {
                    // Parameters map to slots 1..=arity
                    self.local_slots.insert(local, idx);
                }
                LocalClassification::Real => {
                    // Real locals (including non-virtual _0) get slots
                    self.local_slots.insert(local, next_slot);
                    next_slot += 1;
                    slots_to_allocate += 1;
                }
                LocalClassification::Virtual
                | LocalClassification::PhiLike
                | LocalClassification::ReturnPhi
                | LocalClassification::CallResultImmediate
                | LocalClassification::AggregateOperand
                | LocalClassification::CopyOf
                | LocalClassification::Dead => {
                    // Virtual, stack-carried, copy-of, and dead locals don't get slots.
                }
            }
        }

        // VM pre-allocates these slots when entering the frame.
        self.real_local_count = slots_to_allocate;
    }

    /// Get current program counter (next instruction index).
    fn current_pc(&self) -> usize {
        self.bytecode.instructions.len()
    }

    /// Convert a byte offset to a 1-indexed line number.
    fn offset_to_line(&self, offset: u32) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(idx) => idx + 1,
            Err(idx) => idx,
        }
    }

    /// Normalize a span start offset to avoid leading-newline attribution.
    ///
    /// Some statement spans start at the newline byte preceding the real token.
    /// If `start + 1` is a known line start, prefer that offset.
    fn normalize_span_start_offset(&self, start: u32) -> u32 {
        if self.line_starts.binary_search(&(start + 1)).is_ok() {
            start + 1
        } else {
            start
        }
    }

    /// Convert a source span to a display line number.
    ///
    /// Sequence points (statement/terminator boundaries) use normalized start
    /// lines. Non-sequence expression entries fall back to end-line attribution
    /// when a span crosses lines, which avoids collapsing multiline operand
    /// spans to the previous line.
    fn span_to_line(&self, span: Span, sequence_point: bool) -> usize {
        let start: u32 = span.range.start().into();
        let start = self.normalize_span_start_offset(start);
        let start_line = self.offset_to_line(start);

        if sequence_point {
            return start_line;
        }

        let start_u32: u32 = span.range.start().into();
        let end_u32: u32 = span.range.end().into();
        if end_u32 > start_u32 {
            let end_minus_one = end_u32 - 1;
            let end_line = self.offset_to_line(end_minus_one);
            if end_line > start_line && end_line - start_line <= 1 {
                return end_line;
            }
        }

        start_line
    }

    /// Set the current debug span used for subsequent emitted instructions.
    fn set_debug_span(&mut self, span: Option<Span>, sequence_point: bool) {
        self.current_debug_span = span;
        self.pending_sequence_point = sequence_point;
    }

    /// Put back a span saved before pulling operands. A pull that emitted no
    /// instruction has not used the pending sequence point, so it stays.
    fn restore_debug_span(&mut self, span: Option<Span>) {
        self.current_debug_span = span;
    }

    /// Emit a line-table entry for an instruction if needed.
    fn emit_line_table_entry(&mut self, pc: usize) {
        let Some(span) = self.current_debug_span else {
            self.pending_sequence_point = false;
            return;
        };

        let must_emit = match self.bytecode.line_table.last() {
            None => true,
            Some(last) => last.span != span || self.pending_sequence_point,
        };

        if must_emit {
            let line = self.span_to_line(span, self.pending_sequence_point);
            let discriminator = if self.pending_sequence_point {
                let counter = self.next_line_discriminator.entry(line).or_insert(0);
                let out = *counter;
                *counter += 1;
                out
            } else {
                0
            };
            self.bytecode.line_table.push(LineTableEntry {
                pc,
                span,
                line,
                sequence_point: self.pending_sequence_point,
                discriminator,
            });
        }

        self.pending_sequence_point = false;
    }

    /// Emit an instruction and return its index.
    fn emit(&mut self, instruction: Instruction) -> usize {
        let index = self.bytecode.instructions.len();
        self.bytecode.instructions.push(instruction);
        self.bytecode.meta.push(InstructionMeta { operand: None });
        self.emit_line_table_entry(index);
        index
    }

    fn emit_load_var(&mut self, slot: usize) {
        // Superinstruction peepholes (CPython-style, operand-movement only),
        // confined to the current basic block so jump targets / block addresses
        // are never affected:
        //  - StoreVar(slot); LoadVar(slot)  -> StoreVarLoadVar(slot)   (store-keep)
        //  - LoadVar(a);     LoadVar(slot)  -> LoadVar2(a, slot)       (load pair)
        //
        // Skip fusion when a sequence point is pending: the rewrite happens in
        // place on the previous instruction, so it would swallow the new op's
        // sequence point / line entry (the standalone `emit` path below records
        // it). Cheap correctness guard for debugger stepping & line attribution.
        let n = self.bytecode.instructions.len();
        if n > self.current_block_start && !self.pending_sequence_point {
            match self.bytecode.instructions[n - 1] {
                Instruction::StoreVar(prev) if prev == slot => {
                    self.bytecode.instructions[n - 1] = Instruction::StoreVarLoadVar(slot);
                    self.set_var_operand(n - 1, slot);
                    return;
                }
                Instruction::LoadVar(a) => {
                    self.bytecode.instructions[n - 1] = Instruction::LoadVar2(a, slot);
                    return;
                }
                _ => {}
            }
        }

        let inst = self.emit(Instruction::LoadVar(slot));
        self.set_var_operand(inst, slot);
    }

    /// Emit a store to a (non-captured) local slot, folding `StoreVar(a);
    /// StoreVar(slot)` into `StoreVar2(a, slot)` (`STORE_FAST_STORE_FAST`).
    /// In-place rewrite confined to the current basic block, like
    /// [`Self::emit_load_var`].
    fn emit_store_var(&mut self, slot: usize) {
        // See `emit_load_var`: don't fuse across a pending sequence point.
        let n = self.bytecode.instructions.len();
        if n > self.current_block_start && !self.pending_sequence_point {
            if let Instruction::StoreVar(a) = self.bytecode.instructions[n - 1] {
                self.bytecode.instructions[n - 1] = Instruction::StoreVar2(a, slot);
                self.set_var_operand(n - 1, slot);
                return;
            }
        }
        let inst = self.emit(Instruction::StoreVar(slot));
        self.set_var_operand(inst, slot);
    }

    /// Set the resolved operand metadata for an already-emitted instruction.
    fn set_operand(&mut self, index: usize, operand: OperandMeta) {
        self.bytecode.meta[index].operand = Some(operand);
    }

    /// Record the checked argument layout of an already-emitted call.
    fn record_call_layout(&mut self, index: usize, layout: Option<&baml_type::CallLayout>) {
        if let Some(layout) = layout {
            self.bytecode.call_layouts.insert(index, layout.clone());
        }
    }

    /// Set `OperandMeta::Var` for an instruction if the slot has a name.
    fn set_var_operand(&mut self, inst_idx: usize, slot: usize) {
        if let Some(name) = self.slot_names.get(slot).filter(|n| !n.is_empty()) {
            self.set_operand(inst_idx, OperandMeta::Var(name.clone()));
        }
    }

    /// Add a constant to the pool and return its index.
    fn add_constant(&mut self, value: ConstValue) -> usize {
        // Try to find existing constant
        for (i, existing) in self.bytecode.constants.iter().enumerate() {
            if *existing == value {
                return i;
            }
        }
        self.bytecode.constants.push(value);
        self.bytecode.constants.len() - 1
    }

    /// Emit a jump to target, unless it's a fall-through to the next block.
    ///
    /// Applies jump threading: if the target is an empty goto-only block,
    /// jump directly to its final destination instead.
    ///
    /// Returns true if a jump was emitted, false if it was elided.
    fn emit_jump_unless_fallthrough(&mut self, target: BlockId) -> bool {
        let target = self.resolve_pending_target(target);
        // Check if we can fall through to the next emitted block directly.
        let can_fall_through = match target {
            PendingJumpTarget::Block(block_id) => {
                self.next_block.is_some_and(|next| block_id == next)
            }
            PendingJumpTarget::Trap => false,
        };

        if can_fall_through {
            // No jump needed - fall through will get us there
            false
        } else {
            let jump_idx = self.emit(Instruction::Jump(0));
            self.pending_jumps.push((jump_idx, target));
            true
        }
    }

    /// Select polarity from the actual layout, after redirect resolution.
    fn emit_branch(&mut self, then_block: BlockId, else_block: BlockId) {
        let resolved_else = self.resolve_pending_target(else_block);
        if matches!(resolved_else, PendingJumpTarget::Block(block) if Some(block) == self.next_block)
        {
            let target = self.resolve_pending_target(then_block);
            let jump = self.emit(Instruction::PopJumpIfTrue(0));
            self.pending_jumps.push((jump, target));
        } else {
            let jump = self.emit(Instruction::PopJumpIfFalse(0));
            self.pending_jumps.push((jump, resolved_else));
            self.emit_jump_unless_fallthrough(then_block);
        }
    }

    /// Emit an unconditional jump to a target, even when the target is the
    /// next emitted MIR block.
    ///
    /// Switch sub-emitters generate multiple bytecode branches inside a single
    /// MIR terminator before the next MIR block is emitted. In that context,
    /// `next_block` fall-through is not valid for an arm body because later
    /// in-terminator comparison/default code sits between the current PC and
    /// the next MIR block.
    fn emit_jump_always(&mut self, target: BlockId) {
        let target = self.resolve_pending_target(target);
        let jump_idx = self.emit(Instruction::Jump(0));
        self.pending_jumps.push((jump_idx, target));
    }

    /// Resolve a MIR block target into an emitted patch target.
    fn resolve_pending_target(&self, target: BlockId) -> PendingJumpTarget {
        let resolved = self.analysis.resolve_jump_target(target);
        if self.dead_unreachable_blocks.contains(&resolved) {
            PendingJumpTarget::Trap
        } else {
            PendingJumpTarget::Block(resolved)
        }
    }

    /// Ensure a shared trap PC exists if any pending targets require it.
    fn ensure_trap_pc_if_needed(&mut self) {
        if self.trap_pc.is_some() {
            return;
        }
        let needs_trap = self
            .pending_jumps
            .iter()
            .any(|(_, target)| matches!(target, PendingJumpTarget::Trap))
            || self.pending_jump_tables.iter().any(|pending| {
                matches!(pending.otherwise, PendingJumpTarget::Trap)
                    || pending
                        .arms
                        .iter()
                        .any(|(_, target)| matches!(target, PendingJumpTarget::Trap))
            });
        if needs_trap {
            self.set_debug_span(None, false);
            self.trap_pc = Some(self.emit(Instruction::Unreachable));
        }
    }

    // ========================================================================
    // Block Emission
    // ========================================================================

    /// Emit a basic block.
    fn emit_block(&mut self, block: &BasicBlock<'ctx>) {
        // Emit all statements
        for stmt in &block.statements {
            self.set_debug_span(stmt.span, true);
            self.emit_statement(&stmt.kind);
        }

        // Emit terminator
        if let Some(term) = &block.terminator {
            self.set_debug_span(block.terminator_span, true);
            self.emit_terminator(term);
        }
    }

    /// Emit a statement (with virtual assignment skipping).
    fn emit_statement(&mut self, kind: &StatementKind<'ctx>) {
        match kind {
            StatementKind::Assign { destination, value } => {
                // Check if this is an assignment to a Virtual, PhiLike, or Dead local
                if let Place::Local(local) = destination {
                    let class = self.analysis.classifications[local];
                    match pull_semantics::local_assign_behavior(class) {
                        LocalAssignBehavior::Skip => {
                            // Skip! Value will be inlined (Virtual/CopyOf) or discarded (Dead).
                            return;
                        }
                        LocalAssignBehavior::EvalNoStore => {
                            // PhiLike/ReturnPhi: evaluate value and keep it on stack.
                            self.emit_rvalue_pull(value);
                            return;
                        }
                        LocalAssignBehavior::EvalAndStore => {}
                    }
                }

                if self.emit_copy_aware_field_store(destination, value) {
                    return;
                }

                // For field/index stores, push the base object first, then emit the value
                // This sets up the stack correctly for StoreField/StoreArrayElement
                if unwrap_infallible(pull_semantics::walk_projection_store(
                    self,
                    destination,
                    value,
                )) {
                    return;
                }

                match destination {
                    Place::Local(_) | Place::Deref(_) => {
                        // Evaluate the rvalue, then store to the slot or through the cell.
                        self.emit_rvalue_pull(value);
                        self.emit_store_place(destination);
                    }
                    Place::Capture(_) => {
                        unreachable!("a bare capture is a pointer nothing stores to")
                    }
                    Place::Field { .. } | Place::Index { .. } => unreachable!(),
                }
            }
            StatementKind::VirtualFieldStore {
                iface,
                receiver,
                field_index,
                field,
                value,
            } => {
                // Stack: receiver, value, then the interface type — the opcode pops
                // the interface, the value, and the receiver in that order.
                self.emit_operand_pull(receiver);
                self.emit_operand_pull(value);
                let iface_template = self.refs.anchor_template(&iface.to_template());
                let iface_const = self.add_constant(ConstValue::Type(iface_template));
                let inst = self.emit(Instruction::LoadType(iface_const));
                self.set_operand(
                    inst,
                    OperandMeta::Const(self.spelled_template(&iface.to_template())),
                );
                let inst = self.emit(Instruction::VirtualStoreField(*field_index as usize));
                self.set_operand(inst, OperandMeta::Field(field.to_string()));
            }
            StatementKind::Drop(place) => {
                unwrap_infallible(pull_semantics::walk_drop_statement(self, place));
            }
            StatementKind::FreshCell { local, carry_value } => {
                debug_assert!(
                    self.captured_locals.contains(local),
                    "fresh_cell on {local}, which no closure captures"
                );
                let Some(&slot) = self.local_slots.get(local) else {
                    unreachable!("a captured local is always Real");
                };
                if *carry_value {
                    let inst = self.emit(Instruction::LoadDeref(slot));
                    self.set_var_operand(inst, slot);
                } else {
                    let null_idx = self.add_constant(ConstValue::Null);
                    let inst = self.emit(Instruction::LoadConst(null_idx));
                    self.set_operand(inst, OperandMeta::Const("null".to_string()));
                }
                self.emit(Instruction::MakeCell);
                let inst = self.emit(Instruction::StoreVar(slot));
                self.set_var_operand(inst, slot);
            }
            StatementKind::Intrinsic { op, args } => {
                match op {
                    IntrinsicOp::BindType(slot) => {
                        let [value] = args.as_slice() else {
                            unreachable!("`BindType` carries exactly one operand")
                        };
                        self.emit_operand_pull(value);
                        self.emit(Instruction::BindType(*slot as usize));
                    }
                    IntrinsicOp::Log(level) => {
                        // Emit the reserved "$baml_log" event with payload
                        // { level: "<level>", data: <user_arg>, event_name }, where
                        // <user_arg> may be any BAML value.

                        // Save call-site span — walking args may overwrite current_debug_span
                        let call_site_span = self.current_debug_span;

                        // 1. Push event name "$baml_log"
                        let log_str_idx = self.string_object(bex_vm_types::bytecode::LOG_EVENT);
                        let log_const_idx = self
                            .add_constant(ConstValue::Object(ObjectIndex::from_raw(log_str_idx)));
                        let inst = self.emit(Instruction::LoadConst(log_const_idx));
                        self.set_operand(
                            inst,
                            OperandMeta::Const(Self::display_string_operand(
                                bex_vm_types::bytecode::LOG_EVENT,
                            )),
                        );

                        // 2. Push level value string
                        let level_str = match level {
                            LogLevel::Info => "info",
                            LogLevel::Debug => "debug",
                            LogLevel::Warn => "warn",
                            LogLevel::Error => "error",
                        };
                        let level_val_idx = self.string_object(level_str);
                        let level_val_const_idx = self
                            .add_constant(ConstValue::Object(ObjectIndex::from_raw(level_val_idx)));
                        let inst = self.emit(Instruction::LoadConst(level_val_const_idx));
                        self.set_operand(
                            inst,
                            OperandMeta::Const(Self::display_string_operand(level_str)),
                        );

                        // 3. Push user data argument
                        unwrap_infallible(pull_semantics::walk_call_direct_args(self, args));

                        // 4. Push key "level"
                        let level_key_idx = self.string_object("level");
                        let level_key_const_idx = self
                            .add_constant(ConstValue::Object(ObjectIndex::from_raw(level_key_idx)));
                        let inst = self.emit(Instruction::LoadConst(level_key_const_idx));
                        self.set_operand(
                            inst,
                            OperandMeta::Const(Self::display_string_operand("level")),
                        );

                        // 5. Push key "data"
                        let data_key_idx = self.string_object("data");
                        let data_key_const_idx = self
                            .add_constant(ConstValue::Object(ObjectIndex::from_raw(data_key_idx)));
                        let inst = self.emit(Instruction::LoadConst(data_key_const_idx));
                        self.set_operand(
                            inst,
                            OperandMeta::Const(Self::display_string_operand("data")),
                        );

                        let name_key_idx = self.mint_object(Object::String("event_name".into()));
                        let name_key_const_idx = self
                            .add_constant(ConstValue::Object(ObjectIndex::from_raw(name_key_idx)));
                        let inst = self.emit(Instruction::LoadConst(name_key_const_idx));
                        self.set_operand(
                            inst,
                            OperandMeta::Const(Self::display_string_operand("event_name")),
                        );

                        // 6. Push the payload map's key/value type tags, then
                        //    allocate the three-entry event envelope.
                        //    The event is a `map<string, unknown>` (string keys;
                        //    heterogeneous values). The VM's `AllocMap` pops the
                        //    value type (top of stack) then the key type (below it)
                        //    before draining the entries, so push key first, value
                        //    second — mirroring the `alloc_map` helper. Omitting
                        //    these tags makes the VM read the entry keys as types.
                        unwrap_infallible(self.load_type(&TyTemplate::from(RealizedTy::string())));
                        unwrap_infallible(self.load_type(&TyTemplate::from(RealizedTy::unknown())));
                        self.emit(Instruction::AllocMap(3));

                        // 7. Restore call-site span and emit SendEvent
                        self.set_debug_span(call_site_span, true);
                        self.emit(Instruction::SendEvent);
                        // The engine pushes `null` after resuming from SendEvent.
                        // Since this is a statement (not an rvalue), discard it.
                        self.emit(Instruction::Pop(1));
                    }
                }
            }
            StatementKind::Nop => {}
        }
    }

    // ========================================================================
    // Pull-Model Emission
    // ========================================================================

    /// Emit an operand using the pull model.
    ///
    /// For Virtual locals, this recursively emits the definition's rvalue inline.
    /// For Real locals, this emits a `LoadVar` instruction.
    fn emit_operand_pull(&mut self, operand: &Operand<'ctx>) {
        unwrap_infallible(pull_semantics::walk_operand_pull(self, operand));
    }

    fn emit_init_spread(&mut self, fields: Vec<FieldCopy>, display_fields: &[String]) {
        let set_idx = self.bytecode.field_copy_sets.len();
        self.bytecode.field_copy_sets.push(FieldCopySet { fields });
        let inst = self.emit(Instruction::InitSpread(set_idx));
        self.set_operand(inst, OperandMeta::Field(display_fields.join(", ")));
    }

    fn emit_init_instance(&mut self, class: ClassRef<'ctx>, ntypeargs: u16, field_count: usize) {
        let class_name = &baml_compiler2_mir::class_link_name(self.db, class);
        let class_obj_idx = self.refs.class(class);
        let fields = (0..field_count).collect::<Vec<_>>();
        let display_fields = fields
            .iter()
            .map(|field_idx| format!(".{}", self.class_field_name(class, *field_idx)))
            .collect::<Vec<_>>();
        let plan_idx = self.bytecode.class_init_plans.len();
        self.bytecode.class_init_plans.push(ClassInitPlan {
            class_obj: class_obj_idx,
            ntypeargs,
            fields,
        });
        let inst = self.emit(Instruction::InitInstance(plan_idx));
        self.set_operand(
            inst,
            OperandMeta::Object(format!("{class_name} {}", display_fields.join(", "))),
        );
    }

    fn field_copy_operand<'a>(operand: &'a Operand<'_>) -> Option<(&'a Place, usize)> {
        let place = match operand {
            Operand::Copy(place) | Operand::Move(place) => place,
            Operand::Constant(_) => return None,
        };
        let Place::Field { base, field } = place else {
            return None;
        };
        Some((base, *field))
    }

    fn try_emit_class_aggregate_init_instance(
        &mut self,
        class: ClassRef<'ctx>,
        type_arg_templates: &[TyTemplate],
        fields: &[Operand<'ctx>],
    ) -> bool {
        if fields.is_empty()
            || fields
                .iter()
                .any(|field| Self::field_copy_operand(field).is_some())
        {
            return false;
        }

        for field in fields {
            self.emit_operand_pull(field);
        }

        let ntypeargs =
            u16::try_from(type_arg_templates.len()).expect("type_arg_templates count fits in u16");
        for template in type_arg_templates {
            unwrap_infallible(self.load_type(template));
        }
        self.emit_init_instance(class, ntypeargs, fields.len());
        true
    }

    fn place_mentions_stack_carried_local(&self, place: &Place) -> bool {
        match place {
            Place::Local(local) => matches!(
                self.analysis
                    .classifications
                    .get(local)
                    .copied()
                    .unwrap_or(LocalClassification::Real),
                LocalClassification::PhiLike
                    | LocalClassification::ReturnPhi
                    | LocalClassification::CallResultImmediate
                    | LocalClassification::AggregateOperand
            ),
            Place::Field { base, .. } => self.place_mentions_stack_carried_local(base),
            Place::Index { base, index, .. } => {
                self.place_mentions_stack_carried_local(base)
                    || matches!(
                        self.analysis
                            .classifications
                            .get(index)
                            .copied()
                            .unwrap_or(LocalClassification::Real),
                        LocalClassification::PhiLike
                            | LocalClassification::ReturnPhi
                            | LocalClassification::CallResultImmediate
                            | LocalClassification::AggregateOperand
                    )
            }
            Place::Deref(_) | Place::Capture(_) => false,
        }
    }

    fn try_emit_class_aggregate_field_copy_sets(
        &mut self,
        class: ClassRef<'ctx>,
        type_arg_templates: &[TyTemplate],
        fields: &[Operand<'ctx>],
    ) -> bool {
        if !fields
            .iter()
            .any(|field| Self::field_copy_operand(field).is_some())
        {
            return false;
        }

        let ntypeargs =
            u16::try_from(type_arg_templates.len()).expect("type_arg_templates count fits in u16");
        for template in type_arg_templates {
            unwrap_infallible(self.load_type(template));
        }
        self.alloc_instance_of(class, ntypeargs);

        let mut field_idx = 0usize;
        while field_idx < fields.len() {
            let Some((base, source_field)) = Self::field_copy_operand(&fields[field_idx]) else {
                let name = self.class_field_name(class, field_idx);
                self.emit_operand_pull(&fields[field_idx]);
                unwrap_infallible(self.init_field(field_idx, &name));
                field_idx += 1;
                continue;
            };

            if self.place_mentions_stack_carried_local(base) {
                let name = self.class_field_name(class, field_idx);
                self.emit_operand_pull(&fields[field_idx]);
                unwrap_infallible(self.init_field(field_idx, &name));
                field_idx += 1;
                continue;
            }

            let mut copies = vec![FieldCopy {
                source: source_field,
                dest: field_idx,
            }];
            let mut display_fields = vec![format!(".{}", self.class_field_name(class, field_idx))];
            field_idx += 1;

            while field_idx < fields.len() {
                let Some((next_base, next_source_field)) =
                    Self::field_copy_operand(&fields[field_idx])
                else {
                    break;
                };
                if next_base != base || self.place_mentions_stack_carried_local(next_base) {
                    break;
                }
                copies.push(FieldCopy {
                    source: next_source_field,
                    dest: field_idx,
                });
                display_fields.push(format!(".{}", self.class_field_name(class, field_idx)));
                field_idx += 1;
            }

            unwrap_infallible(pull_semantics::walk_place_pull(self, base));
            self.emit_init_spread(copies, &display_fields);
        }

        true
    }

    /// Emit `base.field = base.field <op> rhs` as:
    ///
    /// `base; copy 0; load_field; rhs; op; store_field`
    ///
    /// The generic projection-store path evaluates the destination receiver and
    /// then independently pulls the full rvalue, which re-emits the receiver for
    /// lowered compound assignments. Keeping the receiver on the stack and
    /// duplicating it avoids that second receiver evaluation without changing
    /// the VM's existing `StoreField` stack contract.
    fn emit_copy_aware_field_store(&mut self, destination: &Place, value: &Rvalue<'ctx>) -> bool {
        let Place::Field { base, field } = destination else {
            return false;
        };

        let Rvalue::BinaryOp { op, left, right } = value else {
            return false;
        };

        match left {
            Operand::Copy(place) | Operand::Move(place) if place == destination => {}
            _ => return false,
        }

        let name = self.resolve_field_name(base, *field);
        unwrap_infallible(pull_semantics::walk_place_pull(self, base));
        self.emit(Instruction::Copy(0));
        unwrap_infallible(self.load_field(*field, &name));
        self.emit_operand_pull(right);
        let instruction = self
            .try_specialize_binary_op(*op, left, right)
            .unwrap_or_else(|| Self::binop_instruction(*op));
        self.emit(instruction);
        unwrap_infallible(self.store_field_value(*field, &name));
        true
    }

    /// Emit an rvalue using the pull model.
    fn emit_rvalue_pull(&mut self, rvalue: &Rvalue<'ctx>) {
        // MakeClosure is handled specially: capture operands must load the cell
        // pointer itself (LoadVar), not dereference through the cell (LoadDeref).
        // Set the flag so pull_local emits LoadVar for captured locals.
        if let Rvalue::MakeClosure {
            lambda_idx,
            captures,
            type_arg_templates,
        } = rvalue
        {
            // Emit LoadType for each type-arg template first (not in closure-capture mode).
            for template in type_arg_templates {
                unwrap_infallible(self.load_type(template));
            }
            // Each capture operand is a bare cell pointer: a captured local's
            // slot (`LoadVar`) or one of this closure's captures (`CaptureRef`).
            for capture in captures {
                self.emit_operand_pull(capture);
            }
            unwrap_infallible(self.make_closure_with_type_args(
                *lambda_idx,
                captures.len(),
                type_arg_templates.len(),
            ));
            return;
        }
        if let Rvalue::Aggregate {
            kind:
                baml_compiler2_mir::AggregateKind::Class {
                    class,
                    type_arg_templates,
                },
            fields,
        } = rvalue
        {
            if self.try_emit_class_aggregate_init_instance(*class, type_arg_templates, fields) {
                return;
            }
            if self.try_emit_class_aggregate_field_copy_sets(*class, type_arg_templates, fields) {
                return;
            }
        }
        if let Rvalue::MakeBoundMethod { func, receiver } = rvalue {
            // Emit the receiver onto the stack first.
            self.emit_operand_pull(receiver);
            // Resolve the method to its GlobalIndex.
            let global_idx = self.function_global_index(*func, "MakeBoundMethod: global not found");
            let inst = self.emit(Instruction::MakeBoundMethod(GlobalIndex::from_raw(
                global_idx,
            )));
            self.set_operand(
                inst,
                OperandMeta::Global(baml_compiler2_mir::function_link_name(self.db, *func)),
            );
            return;
        }
        if let Rvalue::MakeVirtualBoundMethod {
            iface,
            method,
            receiver,
            type_args,
        } = rvalue
        {
            // Stack layout mirrors `VirtualCall`: receiver, then the method-level
            // type args, then the interface type (each resolved against the frame
            // by `LoadType`), then the method name — the opcode pops in reverse.
            self.emit_operand_pull(receiver);
            for template in type_args {
                let anchored = self.refs.anchor_template(template);
                let const_idx = self.add_constant(ConstValue::Type(anchored));
                let inst = self.emit(Instruction::LoadType(const_idx));
                self.set_operand(inst, OperandMeta::Const(self.spelled_template(template)));
            }
            let iface_template = self.refs.anchor_template(&iface.to_template());
            let iface_const = self.add_constant(ConstValue::Type(iface_template));
            let inst = self.emit(Instruction::LoadType(iface_const));
            self.set_operand(
                inst,
                OperandMeta::Const(self.spelled_template(&iface.to_template())),
            );
            self.emit_constant(&Constant::String(method.clone()));
            let inst = self.emit(Instruction::MakeVirtualBoundMethod {
                ntypeargs: u16::try_from(type_args.len()).expect("ntypeargs fits in u16"),
            });
            self.set_operand(inst, OperandMeta::Callable(method.clone()));
            return;
        }
        if let Rvalue::MakeVirtualFunction {
            self_ty,
            iface,
            method,
            type_args,
        } = rvalue
        {
            // Stack layout mirrors `MakeVirtualBoundMethod` with the `Self`
            // TYPE in the receiver's slot: `Self`, then the method-level type
            // args (already `Object::Type` OPERANDS — every one of them a
            // `LoadType` temp, a scoped `type T = …` slot included),
            // then the interface type, then the method name — the opcode pops
            // in reverse.
            let self_template = self.refs.anchor_template(self_ty);
            let self_const = self.add_constant(ConstValue::Type(self_template));
            let inst = self.emit(Instruction::LoadType(self_const));
            self.set_operand(inst, OperandMeta::Const(self.spelled_template(self_ty)));
            for arg in type_args {
                self.emit_operand_pull(arg);
            }
            let iface_template = self.refs.anchor_template(&iface.to_template());
            let iface_const = self.add_constant(ConstValue::Type(iface_template));
            let inst = self.emit(Instruction::LoadType(iface_const));
            self.set_operand(
                inst,
                OperandMeta::Const(self.spelled_template(&iface.to_template())),
            );
            self.emit_constant(&Constant::String(method.clone()));
            let inst = self.emit(Instruction::MakeVirtualFunction {
                ntypeargs: u16::try_from(type_args.len()).expect("ntypeargs fits in u16"),
            });
            self.set_operand(inst, OperandMeta::Callable(method.clone()));
            return;
        }
        if let Rvalue::VirtualFieldAccess {
            iface,
            receiver,
            field_index,
            field,
        } = rvalue
        {
            // Stack: receiver, then the interface type (resolved against the frame
            // by `LoadType`) — the opcode pops the interface, then the receiver.
            self.emit_operand_pull(receiver);
            let iface_template = self.refs.anchor_template(&iface.to_template());
            let iface_const = self.add_constant(ConstValue::Type(iface_template));
            let inst = self.emit(Instruction::LoadType(iface_const));
            self.set_operand(
                inst,
                OperandMeta::Const(self.spelled_template(&iface.to_template())),
            );
            let inst = self.emit(Instruction::VirtualLoadField(*field_index as usize));
            self.set_operand(inst, OperandMeta::Field(field.to_string()));
            return;
        }
        // `MakeGenericFunction` needs no special handling here (it has no value
        // captures) — `walk_rvalue_pull` emits it uniformly for both the direct
        // and inlined paths.
        unwrap_infallible(pull_semantics::walk_rvalue_pull(self, rvalue));
    }

    /// The global slot `func` links as, or `None` when this program slots
    /// nothing for it — [`Self::slots`] is the one registry, keyed by
    /// declaration identity on both lanes. A `None` is a callee the program
    /// cannot direct-call: a required interface method (no body), an
    /// intrinsic (never a `Call`), or a served row nothing slots; callers
    /// fall back or panic per their own law. An interface body
    /// reaching codegen unslotted is a loud panic, never a fallback.
    fn try_function_global_index(&mut self, func: FunctionRef<'ctx>) -> Option<usize> {
        let slot = self.refs.function_slot(func).map(GlobalIndex::raw);
        if slot.is_none()
            && let DeclRef::Source(decl) = func
            && baml_compiler2_mir::function_is_interface_body(self.db, decl)
        {
            panic!(
                "interface body has no Pass-1 slot: {}",
                baml_compiler2_mir::function_link_name(self.db, func)
            );
        }
        // A method an impl of a SERVED package provides is slotted by no lane:
        // it is reached only through its impl rule, by dispatch. One arriving
        // here was lowered as a direct reference, which nothing can link.
        if slot.is_none()
            && let DeclRef::External(row) = func
            && row.impl_block(self.db).is_some()
        {
            panic!(
                "internal compiler error: `{}` is provided by an impl of a package served from \
                 its interface, so no lane slots it; it must be reached through its impl rule, \
                 never referenced directly",
                baml_compiler2_mir::function_link_name(self.db, func)
            );
        }
        slot
    }

    /// [`Self::try_function_global_index`], panicking with `what` when the
    /// callable does not resolve.
    fn function_global_index(&mut self, func: FunctionRef<'ctx>, what: &str) -> usize {
        self.try_function_global_index(func).unwrap_or_else(|| {
            panic!(
                "{what}: {}",
                baml_compiler2_mir::function_link_name(self.db, func)
            )
        })
    }

    /// Push a function reference as a value: a pooled, interned
    /// `Object::GenericFunction` wrapper over the function's global slot
    /// (empty `type_args` for a plain reference). Interning by
    /// (function, `type_args`) over the shared object pool makes identical
    /// references share ONE pooled object → pointer-stable identity
    /// (`greet === greet`, `foo<int> === foo<int>`).
    ///
    /// A serial pass scans the package's whole code bucket here, so wrappers
    /// minted by EARLIER functions are reused too. Parallel emit scans only
    /// this worker's fragment; the merge (`merge_item`) replays the
    /// cross-function dedup in original function order, reproducing the
    /// exact serial candidate set and bucket layout.
    fn emit_pooled_function_value(
        &mut self,
        func: FunctionRef<'ctx>,
        type_args: &[baml_compiler2_mir::RealizedTy],
    ) {
        let name_str = baml_compiler2_mir::function_link_name(self.db, func);
        let global_idx = self.function_global_index(func, "undefined function");
        let gidx = GlobalIndex::from_raw(global_idx);
        // The pooled object carries runtime heads; anchor once and compare in
        // that space so an existing instantiation is actually recognized.
        let anchored_args: Box<[bex_vm_types::RealizedTy]> = type_args
            .iter()
            .map(|ty| self.refs.anchor_realized(ty))
            .collect();
        let existing = self
            .objects
            .iter()
            .position(|o| {
                matches!(o, Object::GenericFunction(gf)
                if gf.function == gidx && gf.type_args == anchored_args)
            })
            .map(|local| self.objects_base + local);
        let pool_idx = match existing {
            Some(idx) => idx,
            None => self.mint_object(Object::GenericFunction(bex_vm_types::GenericFunction {
                function: gidx,
                type_args: anchored_args,
                runtime_package: bex_vm_types::HeapPtr::null(),
            })),
        };
        let const_idx = self.add_constant(ConstValue::Object(ObjectIndex::from_raw(pool_idx)));
        let inst = self.emit(Instruction::LoadConst(const_idx));
        let meta = if type_args.is_empty() {
            name_str
        } else {
            format!("{name_str}<...>")
        };
        self.set_operand(inst, OperandMeta::Const(meta));
    }

    fn emit_constant(&mut self, constant: &Constant<'ctx>) {
        match constant {
            Constant::Int(v) => {
                let idx = self.add_constant(ConstValue::Int(*v));
                let inst = self.emit(Instruction::LoadConst(idx));
                self.set_operand(inst, OperandMeta::Const(v.to_string()));
            }
            Constant::Bigint(v) => {
                // Bigints are heap-allocated objects like strings.
                // Push an Object::Bigint into the compile-time objects pool and
                // reference it via ConstValue::Object so that `to_value()` can
                // resolve it to a HeapPtr at load time.
                let operand_str = format!("{v}n");
                let obj_idx = self.mint_object(Object::Bigint(std::sync::Arc::new(v.clone())));
                let const_idx =
                    self.add_constant(ConstValue::Object(ObjectIndex::from_raw(obj_idx)));
                let inst = self.emit(Instruction::LoadConst(const_idx));
                self.set_operand(inst, OperandMeta::Const(operand_str));
            }
            Constant::Float(v) => {
                let idx = self.add_constant(ConstValue::Float(*v));
                let inst = self.emit(Instruction::LoadConst(idx));
                self.set_operand(inst, OperandMeta::Const(bex_vm_types::format_float(*v)));
            }
            Constant::String(s) => {
                let display = Self::display_string_operand(s);
                let obj_idx = self.string_object(s);
                let idx = self.add_constant(ConstValue::Object(ObjectIndex::from_raw(obj_idx)));
                let inst = self.emit(Instruction::LoadConst(idx));
                self.set_operand(inst, OperandMeta::Const(display));
            }
            Constant::Bool(v) => {
                let idx = self.add_constant(ConstValue::Bool(*v));
                let inst = self.emit(Instruction::LoadConst(idx));
                self.set_operand(inst, OperandMeta::Const(v.to_string()));
            }
            Constant::Null => {
                let idx = self.add_constant(ConstValue::Null);
                let inst = self.emit(Instruction::LoadConst(idx));
                self.set_operand(inst, OperandMeta::Const("null".to_string()));
            }
            Constant::OmittedArg => {
                let idx = self.add_constant(ConstValue::OmittedArg);
                let inst = self.emit(Instruction::LoadConst(idx));
                self.set_operand(inst, OperandMeta::Const("<omitted>".to_string()));
            }
            Constant::Function(func) => {
                // A plain function reference as a VALUE. Pooled exactly like
                // `Constant::GenericFunction`, with EMPTY type args: every
                // function-pointer value on the heap is a wrapper object
                // (`GenericFunction`/`Closure`/`BoundMethod`/`HostClosure`),
                // and a raw `Object::Function` is never a data value — the
                // invariant `value_concrete_ty` / `callable_signature` rely
                // on. Interning keeps `greet === greet` pointer-stable, as a
                // direct `LoadGlobal` of the function object did before.
                self.emit_pooled_function_value(*func, &[]);
            }
            Constant::GlobalItem(binding) => {
                // A top-level `let` (a client, ...): read the value `$init`
                // stored in its slot, unwrapped.
                let name_str = baml_compiler2_mir::definition_link_name(
                    self.db,
                    baml_compiler2_hir::contributions::Definition::Let(*binding),
                );
                let global_idx = self.refs.let_global(*binding).raw();
                let inst = self.emit(Instruction::LoadGlobal(GlobalIndex::from_raw(global_idx)));
                self.set_operand(inst, OperandMeta::Global(name_str));
            }
            Constant::GenericFunction { func, type_args } => {
                // `foo<int>` as a value: the same pooled wrapper, carrying its
                // concrete type arguments so calling it seeds `frame.type_args`.
                self.emit_pooled_function_value(*func, type_args);
            }
            Constant::EnumVariant { enum_ref, index } => {
                let enum_name_str = baml_compiler2_mir::enum_link_name(self.db, *enum_ref);
                // TIR admitted the enum and MIR resolved it to a declaration
                // (`enum_ref_of`), so the resolver answers with its object: a
                // local ordinal for this package's own enum, an import
                // ordinal for a dependency's. A miss is an internal error,
                // never a `Null` where a variant belongs.
                let enum_obj_idx = self.refs.enum_(*enum_ref);

                // The discriminant travels in the constant; the name is
                // rendered from the declaration only for operand metadata.
                let variant_str = layout::enum_variants(self.db, *enum_ref)
                    .get(usize::try_from(*index).expect("u32 fits usize"))
                    .map(ToString::to_string)
                    .unwrap_or_else(|| {
                        panic!("internal error: `{enum_name_str}` declares no variant #{index}")
                    });

                let idx = self.add_constant(ConstValue::Int(i64::from(*index)));
                let lc_inst = self.emit(Instruction::LoadConst(idx));
                self.set_operand(
                    lc_inst,
                    OperandMeta::Const(format!("{enum_name_str}.{variant_str}")),
                );
                let inst = self.emit(Instruction::AllocVariant(enum_obj_idx));
                self.set_operand(inst, OperandMeta::Object(enum_name_str));
            }
        }
    }

    // ========================================================================
    // Store Emission
    // ========================================================================

    /// Emit code to store the top-of-stack value to a place.
    ///
    /// Note: Field and Index stores from statements are handled directly in
    /// `emit_statement` to emit base/index before the value. This function
    /// is primarily used for Call/Await destinations which are always locals.
    fn emit_store_place(&mut self, place: &Place) {
        match place {
            Place::Local(local) => {
                let classification = self.analysis.classifications[local];
                match pull_semantics::local_store_behavior(classification) {
                    LocalStoreBehavior::StoreSlot => {
                        debug_assert!(
                            !self.captured_locals.contains(local),
                            "bare store to captured {local}"
                        );
                        // Direct slot store (folds a preceding StoreVar into StoreVar2).
                        self.emit_store_var(self.local_slots[local]);
                    }
                    LocalStoreBehavior::KeepOnStack => {
                        // PhiLike/ReturnPhi: keep value on stack (no-op) - value goes to join/return.
                        // CallResultImmediate: keep value on stack (no-op) - value used immediately.
                    }
                    LocalStoreBehavior::PopValue => {
                        // Virtual, CopyOf, or Dead local - just pop the value
                        self.emit(Instruction::Pop(1));
                    }
                }
            }
            Place::Deref(CellId::Local(local)) => {
                debug_assert!(
                    self.captured_locals.contains(local),
                    "deref of uncaptured {local}"
                );
                self.emit(Instruction::StoreDeref(self.local_slots[local]));
            }
            Place::Deref(CellId::Capture(idx)) => {
                self.emit(Instruction::StoreCapture(*idx));
            }
            Place::Capture(_) => unreachable!("a bare capture is a pointer nothing stores to"),
            Place::Field { .. } | Place::Index { .. } => {
                unreachable!(
                    "Field/Index stores are handled in emit_statement, not emit_store_place"
                );
            }
        }
    }

    /// A type test of a template that names no class and no enum: a tag
    /// where one exactly represents the test, else the value matcher.
    fn is_type_template(&mut self, ty_template: &TyTemplate) {
        let emit_true = |this: &mut Self| {
            this.emit(Instruction::Pop(1));
            let idx = this.add_constant(ConstValue::Bool(true));
            let inst = this.emit(Instruction::LoadConst(idx));
            this.set_operand(inst, OperandMeta::Const("true".to_string()));
        };
        // Hand the whole template to the VM's value matcher
        // (`type_match::value_matches_template`) via a raw `ConstValue::Type`:
        // it resolves the template's frame refs against `frame.type_args` and
        // relates *invariantly* at generic-argument positions — the element- and
        // arg-discriminating check a coarse type tag cannot express (`int[]` ≠
        // `string[]`, `map<string,int>` ≠ `map<string,string>`, a realized `T[]`).
        let emit_structural = |this: &mut Self, template: &TyTemplate| {
            let anchored = this.refs.anchor_template(template);
            let c = this.add_constant(ConstValue::Type(anchored));
            let inst = this.emit(Instruction::IsType(c));
            this.set_operand(inst, OperandMeta::Const(this.spelled_template(template)));
        };
        match ty_template {
            // Lowering states a class test by declaration
            // (`TypeTest::Class`).
            TyTemplate::Class(..) => {
                unreachable!("a class test reaches the emitter as `TypeTest::Class`")
            }

            // ── Structural (value matcher) ───────────────────────────────────
            // A container (element/key/value may discriminate — a coarse tag
            // would conflate `int[]` with `string[]`; the proven-sufficient
            // coarse test is its own `is_type_tag` sink), a bare frame
            // reference (`T`), an interface existential (membership resolved at
            // runtime against the impl registry — never a compile-time
            // implementor enumeration), an associated projection over a frame
            // base (`(#0 as Holder).Item` — `substitute` reduces it through
            // the registry at test time, which is total: every baked rule
            // carries a binding for every declared member, pinned or
            // defaulted), or a union that may carry any of these: the VM
            // value matcher.
            //
            // Media (`image` / `audio` / `video` / `pdf`) belongs here rather
            // than with the tagless leaves below: there is no type tag for
            // media, but `value_concrete_ty` reports the *primitive*
            // `ConcreteRealizedTy::Media(kind)`, so the value matcher
            // discriminates `image` from `audio` exactly. Routing it to the
            // tagless-leaf fallback instead compiles to constant-FALSE — `v is
            // image` false for every value, and a `match`'s media arm never
            // firing (the last arm swallows the value).
            TyTemplate::List(..)
            | TyTemplate::Map { .. }
            | TyTemplate::Future(..)
            | TyTemplate::Media(..)
            | TyTemplate::TypeArgRef(_)
            | TyTemplate::Interface(..)
            | TyTemplate::AssociatedTypeProjection { .. }
            | TyTemplate::Union(..) => emit_structural(self, ty_template),

            // ── Function signatures ──────────────────────────────────────────
            // Signature-precise, via the same value matcher every other
            // structural template uses: it applies the canonical function
            // relation (contravariant parameters, covariant return and
            // throws), and every callable value now reconstructs a faithful
            // function type to compare against — a closure, generic function,
            // or bound method materializes its stored signature templates
            // against the frame it carries. A coarse "is it callable" tag test
            // would answer `true` for a callable of the wrong signature.
            TyTemplate::Function { .. } => emit_structural(self, ty_template),

            // `unknown` is the top type: every value inhabits it, so the test is
            // constant-true. It is a realized *leaf* with no type tag, so without
            // this arm it falls into the tagless-leaf fallback below and compiles
            // to constant-FALSE — silently misrouting every value, not just the
            // valueless ones. (Only refutable positions reach here at all: an
            // exhaustive final `let v: unknown` arm has its test elided.)
            TyTemplate::Unknown => emit_true(self),

            // ── Singleton (literal) ──────────────────────────────────────────
            // A literal type is a set of one, so membership is decided against
            // the value itself, not against a type tag: every tag a literal
            // could name is its *base* type's, which answers `true` for every
            // other inhabitant of that base (`x is One` matching every int when
            // `type One = 1`). `ConstValue::Literal` is the exact test — the
            // specialization of the `ConstValue::Type(Literal)` structural form
            // the algebra would otherwise decide, minus the reconstruction.
            TyTemplate::Literal(literal, _) => {
                let c = self.add_constant(ConstValue::Literal(literal.clone()));
                let inst = self.emit(Instruction::IsType(c));
                self.set_operand(inst, OperandMeta::Const(literal.to_string()));
            }

            // Fully realized leaves keep their exact identity/tag fast path,
            // then use structural matching when no exact fast path exists.
            // This list is exhaustive on purpose: a new template variant must
            // choose its type-test strategy here.
            other @ (TyTemplate::Int
            | TyTemplate::Bigint
            | TyTemplate::Float
            | TyTemplate::String
            | TyTemplate::Bool
            | TyTemplate::Null
            | TyTemplate::Uint8Array
            | TyTemplate::Enum(..)
            | TyTemplate::EnumVariant(..)
            | TyTemplate::RustType
            | TyTemplate::Type
            | TyTemplate::Resource
            | TyTemplate::PromptAst
            | TyTemplate::Void
            | TyTemplate::TypeAlias(..)
            | TyTemplate::Never) => {
                // A fully-realized leaf (primitive, enum, alias, literal, ...):
                // enum-pointer identity for an `Enum`, otherwise its type tag
                // when one exactly represents the test. Tagless leaves use
                // the canonical structural matcher instead of silently
                // compiling to false.
                let realized = <&RealizedTy>::try_from(other)
                    .expect("exhaustive realized-leaf template classification");
                if let RealizedTy::TypeAlias(_) = realized {
                    // Only a RECURSIVE alias survives lowering as a head (a
                    // non-recursive one was expanded); membership in it is
                    // the equirecursive set its definition unfolds to, which
                    // the VM's matcher decides by unfolding the pooled
                    // `Object::TypeAlias` the head binds to at load. An alias
                    // is not a class: the class-object identity test this
                    // arm used to attempt could never find one, so `v is
                    // Tree` compiled to constant false.
                    emit_structural(self, other);
                } else if let RealizedTy::Enum(_) = realized {
                    // Lowering states an enum test by declaration
                    // (`TypeTest::Enum`).
                    unreachable!("an enum test reaches the emitter as `TypeTest::Enum`")
                } else if let Some(tag) = realized_type_tag(realized) {
                    let c = self.add_constant(ConstValue::Int(tag));
                    let inst = self.emit(Instruction::IsType(c));
                    let spelling = baml_compiler2_hir::package::spelling(self.db);
                    self.set_operand(
                        inst,
                        OperandMeta::Const(
                            realized
                                .map_heads(&mut |decl: &DeclName| spelling.wire(decl))
                                .to_string(),
                        ),
                    );
                } else {
                    emit_structural(self, other);
                }
            }
        }
    }

    // ========================================================================
    // Terminator Emission
    // ========================================================================

    fn emit_narrow_bind(&mut self, test: &TypeTest<'ctx>, destination: Local) {
        unwrap_infallible(PullSink::is_type(self, test));
        let last = self
            .bytecode
            .instructions
            .last_mut()
            .expect("is_type emits bytecode");
        if let Instruction::IsType(ty) = *last {
            debug_assert!(!self.captured_locals.contains(&destination));
            *last = Instruction::NarrowBind {
                ty,
                destination: self.local_slots[&destination],
            };
        }
    }

    /// Emit a terminator.
    fn emit_terminator(&mut self, term: &Terminator<'ctx>) {
        match term {
            Terminator::Goto { target } => {
                // Skip jump if target is the next block (fall-through)
                self.emit_jump_unless_fallthrough(*target);
            }

            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                self.emit_operand_pull(condition);
                self.emit_branch(*then_block, *else_block);
            }

            Terminator::NarrowBind {
                source,
                test,
                destination,
                then_block,
                else_block,
            } => {
                self.emit_operand_pull(source);
                self.emit_narrow_bind(test, *destination);
                self.emit_branch(*then_block, *else_block);
            }

            Terminator::Switch {
                discriminant,
                arms,
                otherwise,
                exhaustive,
                arm_names,
            } => {
                // Each arm's key: a kind's fixed tag, or a class declaration
                // by operand.
                let resolved: Vec<(SwitchKey, BlockId)> = arms
                    .iter()
                    .map(|(key, block)| (self.resolve_switch_key(*key), *block))
                    .collect();
                let name_map: std::collections::HashMap<SwitchKey, &str> = arm_names
                    .iter()
                    .map(|(key, name)| (self.resolve_switch_key(*key), name.as_str()))
                    .collect();

                match analyze_switch(&resolved) {
                    SwitchStrategy::Table => {
                        self.emit_switch_table(discriminant, &resolved, *otherwise, &name_map);
                    }
                    strategy @ (SwitchStrategy::JumpTable { .. } | SwitchStrategy::IfElseChain) => {
                        // Value-keyed strategies: every key is a kind.
                        let int_arms: Vec<(i64, BlockId)> = resolved
                            .iter()
                            .map(|(key, block)| (kind_value(*key), *block))
                            .collect();
                        let int_names: std::collections::HashMap<i64, &str> = name_map
                            .iter()
                            .map(|(key, name)| (kind_value(*key), *name))
                            .collect();
                        match strategy {
                            SwitchStrategy::JumpTable { min, max } => self.emit_switch_jump_table(
                                discriminant,
                                &int_arms,
                                *otherwise,
                                min,
                                max,
                                &int_names,
                            ),
                            SwitchStrategy::IfElseChain => self.emit_switch_if_else(
                                discriminant,
                                &int_arms,
                                *otherwise,
                                *exhaustive,
                                &int_names,
                            ),
                            SwitchStrategy::Table => unreachable!("matched above"),
                        }
                    }
                }
            }

            Terminator::Return => {
                // Use pull model for return value - if _0 is Virtual, inline it
                unwrap_infallible(pull_semantics::walk_return_value(self));
                self.emit(Instruction::Return);
            }

            Terminator::Call {
                has_trace,
                argument_layout,
                callee,
                args,
                ntypeargs,

                destination,
                target,
                unwind: _,
            } => {
                let ntypeargs = u16::try_from(*ntypeargs)
                    .unwrap_or_else(|_| unreachable!("a call's type-argument count fits in u16"));
                let call_span = self.current_debug_span;
                let callee_item = pull_semantics::resolve_constant_function_item(
                    callee,
                    &self.analysis.classifications,
                    &self.analysis.def_use,
                );
                let global_callee = callee_item
                    .and_then(|func| self.try_function_global_index(func))
                    .map(GlobalIndex::from_raw);

                if let Some(global_callee) = global_callee {
                    unwrap_infallible(pull_semantics::walk_call_direct_args(self, args));
                    if *has_trace {
                        self.emit(Instruction::SetCallTrace);
                    }

                    let instruction = Instruction::Call {
                        callee: global_callee,
                        ntypeargs,
                    };
                    // Pulling nested argument producers may install their own
                    // debug spans. Restore the terminator's enclosing call span
                    // on the actual call opcode so native diagnostics identify
                    // the offending call rather than its final nested operand.
                    self.set_debug_span(call_span, false);
                    let inst = self.emit(instruction);
                    self.record_call_layout(inst, argument_layout.as_ref());
                    if let Some(func) = callee_item {
                        self.set_operand(
                            inst,
                            OperandMeta::Callable(baml_compiler2_mir::function_link_name(
                                self.db, func,
                            )),
                        );
                    }
                    self.emit_store_place(destination);
                    self.emit_jump_unless_fallthrough(*target);
                } else {
                    // The runtime callee's parameter list is unknown here, so
                    // every lowered indirect call must say what it pushed.
                    assert!(
                        argument_layout.is_some(),
                        "indirect calls require an explicit caller layout"
                    );
                    unwrap_infallible(pull_semantics::walk_call_indirect_operands(
                        self, callee, args, *has_trace,
                    ));
                    if *has_trace {
                        self.emit(Instruction::SetCallTrace);
                    }
                    let instruction = Instruction::CallIndirect;
                    self.set_debug_span(call_span, false);
                    let inst = self.emit(instruction);
                    self.record_call_layout(inst, argument_layout.as_ref());
                    self.emit_store_place(destination);
                    self.emit_jump_unless_fallthrough(*target);
                }
            }

            Terminator::VirtualCall {
                has_trace,
                argument_layout,
                iface,
                method,
                args,
                ntypeargs,
                self_arg,
                destination,
                target,
                unwind: _,
            } => {
                // Push the method type args then the value args (declared
                // order), then the interface type, then the method name — the
                // layout `OpCode::VirtualCall` expects: it pops the method name,
                // then the interface, then the `ntypeargs` method type args, then
                // reads value arg `self_arg` to resolve the impl at runtime.
                let value_args = &args[..args.len() - usize::from(*has_trace)];
                unwrap_infallible(pull_semantics::walk_call_direct_args(self, value_args));
                let iface_template = self.refs.anchor_template(&iface.to_template());
                let iface_const = self.add_constant(ConstValue::Type(iface_template));
                let inst = self.emit(Instruction::LoadType(iface_const));
                self.set_operand(
                    inst,
                    OperandMeta::Const(self.spelled_template(&iface.to_template())),
                );
                self.emit_constant(&Constant::String(method.clone()));
                if *has_trace {
                    self.emit_operand_pull(args.last().expect("trace attachment"));
                    self.emit(Instruction::SetCallTrace);
                }

                let nargs = value_args.len() - ntypeargs;
                let nargs = u16::try_from(nargs)
                    .unwrap_or_else(|_| unreachable!("a call's argument count fits in u16"));
                let ntypeargs = u16::try_from(*ntypeargs)
                    .unwrap_or_else(|_| unreachable!("a call's type-argument count fits in u16"));
                let self_arg = u16::try_from(*self_arg).unwrap_or_else(|_| {
                    unreachable!("a virtual call's dispatch index fits in u16")
                });
                let instruction = Instruction::VirtualCall {
                    nargs,
                    ntypeargs,
                    self_arg,
                };
                let inst = self.emit(instruction);
                self.record_call_layout(inst, argument_layout.as_ref());
                self.set_operand(inst, OperandMeta::Callable(method.clone()));
                self.emit_store_place(destination);
                self.emit_jump_unless_fallthrough(*target);
            }

            Terminator::Unreachable => {
                // Emit an instruction that will panic at runtime if reached.
                // This should never happen - if it does, there's a bug in the
                // compiler or type system (e.g., non-exhaustive match incorrectly
                // marked as exhaustive).
                self.emit(Instruction::Unreachable);
            }

            Terminator::SysOp {
                callee,
                args,
                destination,
                target,
                unwind: _,
            } => {
                let callee_item = pull_semantics::resolve_constant_function_item(
                    callee,
                    &self.analysis.classifications,
                    &self.analysis.def_use,
                );
                let global_callee = callee_item
                    .and_then(|func| self.try_function_global_index(func))
                    .map(GlobalIndex::from_raw)
                    .unwrap_or_else(|| {
                        panic!(
                            "sys_op callee must resolve to a statically-known global function: {callee:?}"
                        )
                    });

                unwrap_infallible(pull_semantics::walk_call_direct_args(self, args));

                let inst = self.emit(Instruction::SysOp(global_callee));
                if let Some(func) = callee_item {
                    self.set_operand(
                        inst,
                        OperandMeta::Callable(baml_compiler2_mir::function_link_name(
                            self.db, func,
                        )),
                    );
                }
                self.emit_store_place(destination);
                self.emit_jump_unless_fallthrough(*target);
            }

            Terminator::Spawn {
                plan,
                future,
                resume,
            } => {
                // `Spawn` pops the plan and pushes the task's future. As for
                // calls: the operand may install nested spans; the spawn
                // opcode belongs to the whole spawn expression.
                let spawn_span = self.current_debug_span;
                self.emit_operand_pull(plan);
                self.restore_debug_span(spawn_span);
                self.emit(Instruction::Spawn);
                self.emit_store_place(future);
                self.emit_jump_unless_fallthrough(*resume);
            }

            Terminator::Await {
                future,
                destination,
                target,
                unwind: _,
            } => {
                let await_span = self.current_debug_span;
                unwrap_infallible(pull_semantics::walk_await_future(self, future));
                self.restore_debug_span(await_span);
                self.emit(Instruction::Await);

                self.emit_store_place(destination);
                self.emit_jump_unless_fallthrough(*target);
            }

            Terminator::AwaitAny {
                futures,
                destination,
                target,
                unwind: _,
            } => {
                // Push the array of futures, then AWAIT_ANY pops it and pushes
                // the winning `int` index (BEP-034 `baml.future.__await_any`).
                self.emit_operand_pull(futures);
                self.emit(Instruction::AwaitAny);

                self.emit_store_place(destination);
                self.emit_jump_unless_fallthrough(*target);
            }

            // The raising opcode belongs to the throw itself, not to the last
            // nested span its operand installed.
            Terminator::Throw { value } => {
                let throw_span = self.current_debug_span;
                self.emit_operand_pull(value);
                self.restore_debug_span(throw_span);
                self.emit(Instruction::Throw);
            }
            Terminator::Rethrow { value, context } => {
                let throw_span = self.current_debug_span;
                self.emit_operand_pull(value);
                self.emit_operand_pull(context);
                self.restore_debug_span(throw_span);
                self.emit(Instruction::Rethrow);
            }

            Terminator::ThrowIfPanic {
                value,
                context,
                otherwise,
            } => {
                let throw_span = self.current_debug_span;
                self.emit_operand_pull(value);
                self.emit_operand_pull(context);
                self.restore_debug_span(throw_span);
                self.emit(Instruction::ThrowIfPanic);
                self.emit_jump_unless_fallthrough(*otherwise);
            }

            Terminator::ShortCircuit {
                operand,
                kind,
                destination,
                eval_rhs,
                join,
            } => {
                // Fused branches retain the value when taken and pop it otherwise.
                //
                // The short-circuit (taken) path leaves the operand value on TOS
                // and jumps to the join. The `eval_rhs` block computes and stores
                // the result via its own trailing `destination = <rhs>` statement.
                // Both paths must agree on where the result lives at the join:
                //
                // * When `destination` is stack-carried, the join consumes the
                //   value straight off TOS, and the `eval_rhs` store is also
                //   elided. The taken path should leave the value on TOS.
                // * Otherwise `destination` is a real slot: the `eval_rhs` store
                //   writes the slot and pops, so the taken path must also store
                //   its TOS value into the slot before the join.
                let store_on_taken_path = !matches!(destination, Place::Local(l)
                    if matches!(
                        pull_semantics::local_store_behavior(self.analysis.classifications[l]),
                        pull_semantics::LocalStoreBehavior::KeepOnStack
                    )
                );
                self.emit_operand_pull(operand);

                let instruction = match kind {
                    baml_compiler2_mir::ShortCircuitKind::And => Instruction::JumpIfFalseOrPop(0),
                    baml_compiler2_mir::ShortCircuitKind::Or => Instruction::JumpIfTrueOrPop(0),
                    baml_compiler2_mir::ShortCircuitKind::Coalesce => {
                        Instruction::JumpIfNotNullOrPop(0)
                    }
                };
                let sc_jump = self.emit(instruction);
                let resolved_join = self.resolve_pending_target(*join);
                if store_on_taken_path {
                    self.emit_jump_always(*eval_rhs);
                    let taken_pc = self.bytecode.instructions.len();
                    self.patch_jump_to(sc_jump, taken_pc);
                    self.emit_store_place(destination);
                    let join_jump = self.emit(Instruction::Jump(0));
                    self.pending_jumps.push((join_jump, resolved_join));
                } else {
                    self.pending_jumps.push((sc_jump, resolved_join));
                    self.emit_jump_unless_fallthrough(*eval_rhs);
                }
            }
        }
    }

    // ========================================================================
    // Jump Patching
    // ========================================================================

    /// Patch all pending jumps with actual addresses.
    fn patch_jumps(&mut self) {
        for (instruction_idx, target) in self.pending_jumps.clone() {
            let target_pc = self.resolve_pending_target_pc(target);
            self.patch_jump_to(instruction_idx, target_pc);
        }
        // A taken value-preserving branch already established the predicate.
        // Thread identical tests without moving PCs or skipping debugger stops.
        for source in 0..self.bytecode.instructions.len() {
            let branch = &self.bytecode.instructions[source];
            let offset = match branch {
                Instruction::JumpIfFalseOrPop(offset)
                | Instruction::JumpIfTrueOrPop(offset)
                | Instruction::JumpIfNotNullOrPop(offset) => *offset,
                _ => continue,
            };
            let mut target = source.wrapping_add_signed(offset);
            while target > source && target < self.bytecode.instructions.len() {
                if self
                    .bytecode
                    .line_table
                    .iter()
                    .any(|entry| entry.pc == target && entry.sequence_point)
                {
                    break;
                }
                let next = &self.bytecode.instructions[target];
                if std::mem::discriminant(next) != std::mem::discriminant(branch) {
                    break;
                }
                let next_offset = match next {
                    Instruction::JumpIfFalseOrPop(offset)
                    | Instruction::JumpIfTrueOrPop(offset)
                    | Instruction::JumpIfNotNullOrPop(offset)
                        if *offset > 0 =>
                    {
                        *offset
                    }
                    _ => break,
                };
                target = target.wrapping_add_signed(next_offset);
            }
            self.patch_jump_to(source, target);
        }
    }

    /// Resolve a pending jump target to a concrete bytecode PC.
    fn resolve_pending_target_pc(&self, target: PendingJumpTarget) -> usize {
        match target {
            PendingJumpTarget::Block(target_block) => {
                *self.block_addresses.get(&target_block).unwrap_or_else(|| {
                    panic!(
                        "missing block address for jump target {target_block:?}; target may have been skipped without redirect resolution"
                    )
                })
            }
            PendingJumpTarget::Trap => self.trap_pc.unwrap_or_else(|| {
                panic!("missing trap PC for dead-unreachable jump target")
            }),
        }
    }

    /// Patch a specific jump to a specific destination.
    #[allow(clippy::cast_possible_wrap)]
    fn patch_jump_to(&mut self, instruction_idx: usize, destination: usize) {
        let offset = destination as isize - instruction_idx as isize;
        match self.bytecode.instructions[instruction_idx] {
            Instruction::Jump(_) => {
                self.bytecode.instructions[instruction_idx] = Instruction::Jump(offset);
            }
            Instruction::PopJumpIfFalse(_) => {
                self.bytecode.instructions[instruction_idx] = Instruction::PopJumpIfFalse(offset);
            }
            Instruction::JumpIfFalse(_) => {
                self.bytecode.instructions[instruction_idx] = Instruction::JumpIfFalse(offset);
            }
            Instruction::PopJumpIfTrue(_) => {
                self.bytecode.instructions[instruction_idx] = Instruction::PopJumpIfTrue(offset);
            }
            Instruction::JumpIfFalseOrPop(_) => {
                self.bytecode.instructions[instruction_idx] = Instruction::JumpIfFalseOrPop(offset);
            }
            Instruction::JumpIfTrueOrPop(_) => {
                self.bytecode.instructions[instruction_idx] = Instruction::JumpIfTrueOrPop(offset);
            }
            Instruction::JumpIfNotNullOrPop(_) => {
                self.bytecode.instructions[instruction_idx] =
                    Instruction::JumpIfNotNullOrPop(offset);
            }
            _ => panic!("expected jump instruction at index {instruction_idx}"),
        }
    }

    /// Patch all pending jump tables with actual offsets.
    #[allow(clippy::cast_possible_wrap)]
    fn patch_jump_tables(&mut self) {
        for pending in std::mem::take(&mut self.pending_jump_tables) {
            let jump_table_pc = pending.jump_table_pc;
            let mut table = pending.table;

            // Patch each arm's offset
            for (value, target) in &pending.arms {
                let target_pc = self.resolve_pending_target_pc(*target);
                let offset = target_pc as isize - jump_table_pc as isize;
                table.set(*value, offset);
            }

            // Patch default offset
            let otherwise_pc = self.resolve_pending_target_pc(pending.otherwise);
            let default_offset = otherwise_pc as isize - jump_table_pc as isize;

            // Store default in the table metadata, not in the instruction
            table.default = default_offset;

            // Update the instruction to reference the final table index
            self.bytecode.instructions[jump_table_pc] = Instruction::JumpTable(pending.table_idx);

            // Store the completed table
            self.bytecode.jump_tables.push(table);
        }
    }

    /// Build the bytecode exception table from the blocks' handler edges.
    ///
    /// Every block names the handler a throw or panic inside it unwinds to
    /// (`BasicBlock::unwind`), so each block contributes one entry over its
    /// exact PC range (adjacent ranges to the same handler are coalesced).
    /// Coverage therefore does not depend on layout: a direct-throw block
    /// that sinks past its handler, or a call-free panic-capable block with
    /// no unwind edge to anchor it, is covered by its own range. Entries
    /// never overlap — a block has one handler — so the VM's innermost-entry
    /// selection has exactly one candidate at any PC.
    ///
    /// `BasicBlock::handling` gives one `HandlerContextEntry` per block the
    /// same way, for the BEP-042 cause chain: a throw there is "during
    /// handling of" the error whose context lives in that handler's landing.
    fn build_exception_table(&mut self, mir: &MirFunctionBody<'ctx>) {
        use bex_vm_types::bytecode::{ExceptionTableEntry, HandlerContextEntry};

        // (start, end, handler) per emitted, non-empty block.
        let mut protected: Vec<(usize, usize, BlockId)> = Vec::new();
        let mut handling: Vec<(usize, usize, BlockId)> = Vec::new();
        for block in &mir.blocks {
            let (Some(&start), Some(&end)) = (
                self.block_addresses.get(&block.id),
                self.block_end_addresses.get(&block.id),
            ) else {
                continue; // block dropped by layout / DCE
            };
            if start >= end {
                continue; // empty block — nothing to cover
            }
            if let Some(handler) = block.unwind {
                protected.push((start, end, handler));
            }
            if let Some(handler) = block.handling {
                handling.push((start, end, handler));
            }
        }
        protected.sort_unstable_by_key(|&(start, end, _)| (start, end));
        let mut coalesced: Vec<(usize, usize, BlockId)> = Vec::new();
        for (start, end, handler) in protected {
            match coalesced.last_mut() {
                Some(last) if last.2 == handler && start <= last.1 => last.1 = last.1.max(end),
                _ => coalesced.push((start, end, handler)),
            }
        }
        for (start_pc, end_pc, handler) in coalesced {
            let (handler_pc, error_slot, context_slot) = self.landing_slots(mir, handler);
            self.bytecode.exception_table.push(ExceptionTableEntry {
                start_pc,
                end_pc,
                handler_pc,
                error_slot,
                context_slot,
            });
        }
        for (start_pc, end_pc, handler) in handling {
            let (handler_pc, _, context_slot) = self.landing_slots(mir, handler);
            self.bytecode
                .handler_context_table
                .push(HandlerContextEntry {
                    start_pc,
                    end_pc,
                    handler_pc,
                    context_slot,
                });
        }
        self.bytecode.exception_table.sort_by_key(|e| e.start_pc);
    }

    /// Build the bytecode's shield table from the blocks stamped `shielded`
    /// (the bodies of `defer`s): one exact PC range per emitted block,
    /// coalesced where the layout put them back-to-back, sorted by start.
    fn build_shield_table(&mut self, mir: &MirFunctionBody<'ctx>) {
        use bex_vm_types::bytecode::ShieldRange;

        let mut ranges: Vec<(usize, usize)> = mir
            .blocks
            .iter()
            .filter(|block| block.shielded)
            .filter_map(|block| {
                let &start = self.block_addresses.get(&block.id)?;
                let &end = self.block_end_addresses.get(&block.id)?;
                (start < end).then_some((start, end))
            })
            .collect();
        ranges.sort_unstable();
        for (start_pc, end_pc) in ranges {
            match self.bytecode.shield_table.last_mut() {
                Some(last) if start_pc <= last.end_pc => last.end_pc = last.end_pc.max(end_pc),
                _ => self
                    .bytecode
                    .shield_table
                    .push(ShieldRange { start_pc, end_pc }),
            }
        }
    }

    /// Where the VM lands for `handler`: its PC and the slots of its error
    /// and context locals (landing locals are always `Real`).
    fn landing_slots(
        &self,
        mir: &MirFunctionBody<'ctx>,
        handler: BlockId,
    ) -> (usize, usize, usize) {
        let landing = mir
            .block(handler)
            .landing
            .unwrap_or_else(|| unreachable!("{handler:?} is not a handler block"));
        let resolved = self.analysis.resolve_jump_target(handler);
        let &handler_pc = self.block_addresses.get(&resolved).unwrap_or_else(|| {
            unreachable!(
                "exception table: handler block {handler:?} has no PC address — \
                 a block unwinds to it but it was dropped"
            )
        });
        let slot_of = |local: Local| {
            *self.local_slots.get(&local).unwrap_or_else(|| {
                unreachable!("landing local {local:?} of handler {handler:?} has no slot")
            })
        };
        (
            handler_pc,
            slot_of(landing.error_local),
            slot_of(landing.context_local),
        )
    }

    // ========================================================================
    // Switch Emission Strategies
    // ========================================================================

    /// Emit switch using if-else chain (O(n) comparisons).
    ///
    /// This is the original linear emission strategy.
    ///
    /// If `exhaustive` is true, the last arm's comparison is skipped since
    /// if all previous comparisons failed, the discriminant must match.
    fn emit_switch_if_else(
        &mut self,
        discriminant: &Operand<'ctx>,
        arms: &[(i64, BlockId)],
        otherwise: BlockId,
        exhaustive: bool,
        name_map: &std::collections::HashMap<i64, &str>,
    ) {
        // Single exhaustive arm: no comparison needed, skip the discriminant entirely.
        if exhaustive && arms.len() == 1 {
            self.emit_jump_unless_fallthrough(arms[0].1);
            return;
        }

        // Each arm re-loads the discriminant from the operand instead of
        // keeping it on the stack with copy/pop. This makes each arm
        // self-contained and avoids stack cleanup instructions.
        let num_arms = arms.len();
        for (i, (value, target)) in arms.iter().enumerate() {
            let is_last = i == num_arms - 1;

            // For exhaustive switches, skip the last arm's comparison.
            if exhaustive && is_last {
                self.emit_jump_unless_fallthrough(*target);
                return;
            }

            let label = Self::switch_label(*value, name_map);
            self.emit_operand_pull(discriminant);
            let idx = self.add_constant(ConstValue::Int(*value));
            let inst = self.emit(Instruction::LoadConst(idx));
            self.set_operand(inst, OperandMeta::Const(label));
            self.emit(Instruction::CmpIntOp(CmpOp::Eq));
            let jump_idx = self.emit(Instruction::PopJumpIfFalse(0));
            self.emit_jump_always(*target);
            let skip_to = self.current_pc();
            self.patch_jump_to(jump_idx, skip_to);
        }

        self.emit_jump_unless_fallthrough(otherwise);
    }

    /// Emit switch using jump table (O(1) lookup).
    ///
    /// Creates a jump table for dense integer ranges.
    fn emit_switch_jump_table(
        &mut self,
        discriminant: &Operand<'ctx>,
        arms: &[(i64, BlockId)],
        otherwise: BlockId,
        min: i64,
        max: i64,
        name_map: &std::collections::HashMap<i64, &str>,
    ) {
        // 1. Push discriminant onto stack
        self.emit_operand_pull(discriminant);

        // 2. Create jump table data structure with placeholder offsets
        let table_idx = self.pending_jump_tables.len();
        let mut table = JumpTableData::new(min, max);

        // Populate symbolic names from arm_names
        for (&value, &name) in name_map {
            table.set_name(value, name.to_string());
        }

        // Resolve all jump targets through redirect threading so we don't retain
        // references to skipped redirect-source blocks.
        let resolved_arms: Vec<(i64, PendingJumpTarget)> = arms
            .iter()
            .map(|(value, target)| (*value, self.resolve_pending_target(*target)))
            .collect();
        let resolved_otherwise = self.resolve_pending_target(otherwise);

        // 3. Emit JumpTable instruction (default is stored in JumpTableData, patched later)
        let jump_table_pc = self.emit(Instruction::JumpTable(table_idx));

        // 4. Record pending jump table for patching
        self.pending_jump_tables.push(PendingJumpTable {
            table_idx,
            jump_table_pc,
            arms: resolved_arms,
            otherwise: resolved_otherwise,
            table,
        });
    }

    /// Emit switch through a switch table + dense jump table.
    ///
    /// `DenseTag` remaps the sparse value to the dense `[0, K-1]` index of
    /// its arm through the table; a subsequent `JumpTable` dispatches on the
    /// dense index. `DenseTag` pushes `-1` for a value no key matches, which
    /// falls to the jump table's default arm.
    ///
    /// The table states the keys in arm order and nothing else: whoever lays
    /// the unit into an image solves it
    /// ([`SwitchDispatch::solved`](bex_vm_types::bytecode::SwitchDispatch::solved))
    /// after relocating the keys, so a kind key and a declaration key take
    /// one road.
    #[allow(clippy::cast_possible_wrap)]
    fn emit_switch_table(
        &mut self,
        discriminant: &Operand<'ctx>,
        arms: &[(SwitchKey, BlockId)],
        otherwise: BlockId,
        name_map: &std::collections::HashMap<SwitchKey, &str>,
    ) {
        // 1. Push discriminant (type tag) onto stack — consumed by DenseTag.
        self.emit_operand_pull(discriminant);

        // 2. Store the table in bytecode and emit the DenseTag instruction.
        let label = |key: &SwitchKey| -> String {
            name_map
                .get(key)
                .map(ToString::to_string)
                .unwrap_or_else(|| match key {
                    SwitchKey::Kind(value) => value.to_string(),
                    SwitchKey::Declaration(declaration) => format!("class @{}", declaration.raw()),
                })
        };
        let table = SwitchTable::of_keys(
            arms.iter().map(|(key, _)| *key).collect(),
            arms.iter().map(|(key, _)| label(key)).collect(),
        );
        let switch_table_idx = self.bytecode.switch_tables.len();
        self.bytecode.switch_tables.push(table);
        self.emit(Instruction::DenseTag(switch_table_idx));

        // 3. Emit a dense JumpTable over [0, K-1].
        //    The DenseTag output is dense by construction, so this is always
        //    a compact table with no holes. We emit the JumpTable directly
        //    (not via emit_switch_jump_table) because the dense index is
        //    already on the stack from DenseTag — we must not re-push the
        //    original discriminant.
        let k = arms.len();
        let dense_min = 0i64;
        let dense_max = (k - 1) as i64;

        let jt_table_idx = self.pending_jump_tables.len();
        let mut jt = JumpTableData::new(dense_min, dense_max);

        // Set symbolic names from the original arm names.
        for (dense_idx, (key, _)) in arms.iter().enumerate() {
            if let Some(&name) = name_map.get(key) {
                jt.set_name(dense_idx as i64, name.to_string());
            }
        }

        // Build dense arm mapping: dense_index → original BlockId.
        let resolved_arms: Vec<(i64, PendingJumpTarget)> = arms
            .iter()
            .enumerate()
            .map(|(dense_idx, (_, target))| {
                (dense_idx as i64, self.resolve_pending_target(*target))
            })
            .collect();
        let resolved_otherwise = self.resolve_pending_target(otherwise);

        let jump_table_pc = self.emit(Instruction::JumpTable(jt_table_idx));

        self.pending_jump_tables.push(PendingJumpTable {
            table_idx: jt_table_idx,
            jump_table_pc,
            arms: resolved_arms,
            otherwise: resolved_otherwise,
            table: jt,
        });
    }

    // ========================================================================
    // Switch helpers
    // ========================================================================

    /// Resolve a label for a switch arm value from the name map,
    /// falling back to the integer's string representation.
    fn switch_label(value: i64, name_map: &std::collections::HashMap<i64, &str>) -> String {
        name_map
            .get(&value)
            .map(|n| (*n).to_string())
            .unwrap_or_else(|| value.to_string())
    }

    // ========================================================================
    // Helpers
    // ========================================================================

    /// Convert MIR `BinOp` to VM instruction.
    fn binop_instruction(op: BinOp) -> Instruction {
        match op {
            BinOp::Add => Instruction::BinOp(VmBinOp::Add),
            BinOp::Sub => Instruction::BinOp(VmBinOp::Sub),
            BinOp::Mul => Instruction::BinOp(VmBinOp::Mul),
            BinOp::Div => Instruction::BinOp(VmBinOp::Div),
            BinOp::Mod => Instruction::BinOp(VmBinOp::Mod),
            BinOp::Eq => Instruction::CmpOp(CmpOp::Eq),
            BinOp::Ne => Instruction::CmpOp(CmpOp::NotEq),
            BinOp::Lt => Instruction::CmpOp(CmpOp::Lt),
            BinOp::Le => Instruction::CmpOp(CmpOp::LtEq),
            BinOp::Gt => Instruction::CmpOp(CmpOp::Gt),
            BinOp::Ge => Instruction::CmpOp(CmpOp::GtEq),
            BinOp::BitAnd => Instruction::BinOp(VmBinOp::BitAnd),
            BinOp::BitOr => Instruction::BinOp(VmBinOp::BitOr),
            BinOp::BitXor => Instruction::BinOp(VmBinOp::BitXor),
            BinOp::Shl => Instruction::BinOp(VmBinOp::Shl),
            BinOp::Shr => Instruction::BinOp(VmBinOp::Shr),
        }
    }

    /// Convert MIR `UnaryOp` to VM instruction.
    fn unaryop_instruction(op: UnaryOp) -> Instruction {
        match op {
            UnaryOp::Not => Instruction::UnaryOp(VmUnaryOp::Not),
            UnaryOp::Neg => Instruction::UnaryOp(VmUnaryOp::Neg),
            UnaryOp::Truthy => Instruction::UnaryOp(VmUnaryOp::Truthy),
        }
    }

    /// Build local variable name mapping from MIR and slot assignments.
    ///
    /// Returns a flat `Vec<String>` mapping slot indices to variable names.
    fn build_local_names(
        mir: &MirFunctionBody<'ctx>,
        local_slots: &HashMap<Local, usize>,
    ) -> Vec<String> {
        let max_slot = local_slots.values().max().copied().unwrap_or(0);
        let mut names = vec![String::new(); max_slot + 1];

        for (&local, &slot) in local_slots {
            let local_decl = mir.local(local);
            let name = local_decl
                .name
                .as_ref()
                .map(std::string::ToString::to_string)
                .unwrap_or_else(|| format!("_{}", local.0));
            names[slot] = name;
        }

        names
    }

    /// Build lexical-scope metadata for user-visible locals.
    fn build_debug_locals(
        mir: &MirFunctionBody<'ctx>,
        local_slots: &HashMap<Local, usize>,
    ) -> Vec<DebugLocalScope> {
        let mut locals = Vec::new();

        for (&local, &slot) in local_slots {
            let decl = mir.local(local);
            let Some(name) = decl.name.as_ref() else {
                continue;
            };
            let Some(scope_span) = decl.scope_span else {
                continue;
            };
            if name.as_str() == "_" {
                continue;
            }
            locals.push(DebugLocalScope {
                slot,
                name: name.to_string(),
                scope_span,
            });
        }

        locals.sort_by(|a, b| {
            (
                a.scope_span.file_id.as_u32(),
                u32::from(a.scope_span.range.start()),
                a.slot,
            )
                .cmp(&(
                    b.scope_span.file_id.as_u32(),
                    u32::from(b.scope_span.range.start()),
                    b.slot,
                ))
        });

        locals
    }

    /// Emit a `MakeClosure` bytecode instruction with the given counts.
    ///
    /// This is the underlying implementation called by both the `PullSink`
    /// trait methods (`make_closure` and `make_closure_with_type_args`).
    fn emit_make_closure_bytecode(
        &mut self,
        lambda_idx: usize,
        capture_count: usize,
        ntypeargs: usize,
    ) {
        let obj_idx = *self
            .lambda_object_indices
            .get(lambda_idx)
            .unwrap_or_else(|| panic!("make_closure: lambda_idx {lambda_idx} out of range"));
        let name = self
            .lambda_names
            .get(lambda_idx)
            .cloned()
            .unwrap_or_else(|| format!("<lambda {lambda_idx}>"));
        let inst = self.emit(Instruction::MakeClosure {
            obj_idx: ObjectIndex::from_raw(obj_idx),
            capture_count,
            ntypeargs,
        });
        self.set_operand(inst, OperandMeta::Object(name));
    }
}

impl<'db: 'ctx, 'ctx> PullSink<'ctx> for StackifyCodegen<'db, 'ctx, '_, '_> {
    type Error = Infallible;

    fn pull_constant(&mut self, constant: &Constant<'ctx>) -> Result<(), Self::Error> {
        self.emit_constant(constant);
        Ok(())
    }

    fn pull_local(&mut self, local: Local) -> Result<LocalPullAction<'ctx>, Self::Error> {
        let classification = self.analysis.classifications[&local];

        let action = match classification {
            LocalClassification::Virtual => {
                // Attribute inlined virtual loads to their defining statement,
                // then give the consumer back its own span: the instruction that
                // uses this operand (a division, a call) belongs to the consuming
                // expression, not to its last inlined operand.
                let consumer_span = self.current_debug_span;
                self.set_debug_span(self.def_span_for_local(local), false);
                // Inline the definition rvalue at use site.
                let rvalue = self.analysis.def_use[&local]
                    .def
                    .as_ref()
                    .map(|def| def.rvalue.clone())
                    .unwrap_or_else(|| panic!("virtual local {local} without definition"));
                // MakeClosure, MakeBoundMethod, MakeVirtualBoundMethod, and
                // VirtualFieldAccess are materialized only by `emit_rvalue_pull`
                // (`walk_rvalue_pull` panics on them), so route through it.
                // Class aggregates may use emitter-only spread helpers, so they
                // also need to flow through `emit_rvalue_pull` when inlined.
                if matches!(
                    rvalue,
                    Rvalue::MakeClosure { .. }
                        | Rvalue::MakeBoundMethod { .. }
                        | Rvalue::MakeVirtualBoundMethod { .. }
                        | Rvalue::MakeVirtualFunction { .. }
                        | Rvalue::VirtualFieldAccess { .. }
                        | Rvalue::Aggregate {
                            kind: baml_compiler2_mir::AggregateKind::Class { .. },
                            ..
                        }
                ) {
                    self.emit_rvalue_pull(&rvalue);
                } else {
                    pull_semantics::walk_rvalue_pull(self, &rvalue)?;
                }
                self.restore_debug_span(consumer_span);
                LocalPullAction::Done
            }
            LocalClassification::PhiLike
            | LocalClassification::ReturnPhi
            | LocalClassification::CallResultImmediate
            | LocalClassification::AggregateOperand => LocalPullAction::Done,
            LocalClassification::CopyOf => {
                // Copy propagation: load from source slot directly.
                let source = self.analysis.resolve_copy_source(local);
                debug_assert!(
                    !self.captured_locals.contains(&source),
                    "copy of captured {source}"
                );
                self.emit_load_var(self.local_slots[&source]);
                LocalPullAction::Done
            }
            LocalClassification::Parameter
            | LocalClassification::Real
            | LocalClassification::Dead => {
                // The slot's value; for a captured local that is its cell
                // pointer, which only a closure capture operand reads bare.
                self.emit_load_var(self.local_slots[&local]);
                LocalPullAction::Done
            }
        };

        Ok(action)
    }

    fn load_field(&mut self, field: usize, name: &str) -> Result<(), Self::Error> {
        let idx = self.emit(Instruction::LoadField(field));
        self.set_operand(idx, OperandMeta::Field(name.to_string()));
        Ok(())
    }

    fn load_index(&mut self, kind: IndexKind) -> Result<(), Self::Error> {
        match kind {
            IndexKind::Array => {
                self.emit(Instruction::LoadArrayElement);
            }
            IndexKind::Map => {
                self.emit(Instruction::LoadMapElement);
            }
        }
        Ok(())
    }

    /// Select a type-specialized binary instruction when operand types permit it.
    fn binary_op(
        &mut self,
        op: BinOp,
        left: &Operand<'ctx>,
        right: &Operand<'ctx>,
    ) -> Result<(), Self::Error> {
        let instruction = self
            .try_specialize_binary_op(op, left, right)
            .unwrap_or_else(|| Self::binop_instruction(op));
        self.emit(instruction);
        Ok(())
    }

    fn unary_op(&mut self, op: UnaryOp) -> Result<(), Self::Error> {
        self.emit(Self::unaryop_instruction(op));
        Ok(())
    }

    fn alloc_array(&mut self, element_ty: &TyTemplate, len: usize) -> Result<(), Self::Error> {
        // Push the (frame-resolved) element type on top of the `len` elements;
        // the VM's `AllocArray` pops it before draining the values, mirroring how
        // `AllocInstance` consumes its leading type args.
        self.load_type(element_ty)?;
        self.emit(Instruction::AllocArray(len));
        Ok(())
    }

    fn alloc_uint8array(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        use std::fmt::Write;
        // Store the byte data as a compile-time constant template, then deep-copy
        // it to produce a mutable TLAB allocation (matching array literal semantics).
        let mut display = String::from("b\"");
        for b in bytes {
            write!(display, "\\x{b:02x}").unwrap();
        }
        display.push('"');
        let obj_idx = self.mint_object(Object::Uint8Array(bytes.to_vec().into()));
        let idx = self.add_constant(ConstValue::Object(ObjectIndex::from_raw(obj_idx)));
        let inst = self.emit(Instruction::LoadConst(idx));
        self.set_operand(inst, OperandMeta::Const(display));
        let deep_copy = baml_compiler2_hir_ty::callable::lang_function(
            self.db,
            baml_base::LangPackage::Baml,
            &[],
            "deep_copy",
        )
        .unwrap_or_else(|| {
            panic!("internal compiler error: the stdlib exports no `baml.deep_copy`")
        });
        let deep_copy_idx = self.function_global_index(deep_copy, "undefined function");
        let inst = self.emit(Instruction::Call {
            callee: GlobalIndex::from_raw(deep_copy_idx),
            ntypeargs: 0,
        });
        self.set_operand(
            inst,
            OperandMeta::Callable(baml_compiler2_mir::function_link_name(self.db, deep_copy)),
        );
        Ok(())
    }

    fn alloc_map(
        &mut self,
        key_ty: &TyTemplate,
        value_ty: &TyTemplate,
        len: usize,
    ) -> Result<(), Self::Error> {
        // Push key then value type on top of the entries; the VM's `AllocMap`
        // pops value then key before processing the pairs.
        self.load_type(key_ty)?;
        self.load_type(value_ty)?;
        self.emit(Instruction::AllocMap(len));
        Ok(())
    }

    fn alloc_class_instance(
        &mut self,
        class: ClassRef<'ctx>,
        ntypeargs: u16,
    ) -> Result<(), Self::Error> {
        self.alloc_instance_of(class, ntypeargs);
        Ok(())
    }

    fn init_class_instance(
        &mut self,
        class: ClassRef<'ctx>,
        ntypeargs: u16,
        field_count: usize,
    ) -> Result<(), Self::Error> {
        self.emit_init_instance(class, ntypeargs, field_count);
        Ok(())
    }

    fn init_field(&mut self, field_idx: usize, name: &str) -> Result<(), Self::Error> {
        let idx = self.emit(Instruction::InitField(field_idx));
        self.set_operand(idx, OperandMeta::Field(name.to_string()));
        Ok(())
    }

    fn discriminant(&mut self) -> Result<(), Self::Error> {
        self.emit(Instruction::Discriminant);
        Ok(())
    }

    fn type_tag(&mut self) -> Result<(), Self::Error> {
        self.emit(Instruction::TypeTag);
        Ok(())
    }

    fn len_of_place(&mut self, place: &Place) -> Result<(), Self::Error> {
        // MIR `Rvalue::Len` → dedicated ContainerLen opcode (no function call overhead).
        pull_semantics::walk_place_pull(self, place)?;
        self.emit(Instruction::ContainerLen);
        Ok(())
    }

    fn is_type(&mut self, test: &TypeTest<'ctx>) -> Result<(), Self::Error> {
        match test {
            // ── Class check ──────────────────────────────────────────────────
            // Every class (monomorphic `Foo`, concrete `Foo<int>`, or generic
            // `Foo<T>`) is tested against the declaration lowering resolved.
            // Non-empty args → `ClassWithTypeArgs` so the VM compares each arg
            // invariantly; empty args → class-pointer identity.
            TypeTest::Class { class, args } => {
                let class_name = self.spelled_head(&layout::class_head(self.db, *class));
                let class_obj = self.refs.class(*class);
                if args.is_empty() {
                    let c = self.add_constant(ConstValue::Object(class_obj));
                    let inst = self.emit(Instruction::IsType(c));
                    self.set_operand(inst, OperandMeta::Const(class_name));
                } else {
                    let type_args_templates = args
                        .iter()
                        .map(|template| self.refs.anchor_template(template))
                        .collect();
                    let c = self.add_constant(ConstValue::ClassWithTypeArgs {
                        class_obj,
                        type_args_templates,
                    });
                    let inst = self.emit(Instruction::IsType(c));
                    self.set_operand(inst, OperandMeta::Const(format!("{class_name}<...>")));
                }
            }
            // Enum-pointer identity: `is Color` tests the value's enum object,
            // so it discriminates `Color` from `Status` — the shared `ENUM`
            // type tag cannot.
            TypeTest::Enum(enum_ref) => {
                let enum_name = self.spelled_head(&layout::enum_head(self.db, *enum_ref));
                let enum_obj = self.refs.enum_(*enum_ref);
                let c = self.add_constant(ConstValue::Object(enum_obj));
                let inst = self.emit(Instruction::IsType(c));
                self.set_operand(inst, OperandMeta::Const(enum_name));
            }
            TypeTest::Template(template) => self.is_type_template(template),
        }
        Ok(())
    }

    fn is_type_tag(&mut self, tag: i64) -> Result<(), Self::Error> {
        // The proven coarse-tag test: identical `IsType`-against-`Int` bytecode
        // to the tag checks `is_type` emits for realized leaves. The operand
        // meta reproduces the strings the wildcarded container templates used
        // to render (`_[]` / `map<_, _>`) so bytecode display stays stable
        // across the `IsTypeTag` re-home; other tags have no MIR producer.
        let c = self.add_constant(ConstValue::Int(tag));
        let inst = self.emit(Instruction::IsType(c));
        let meta = match tag {
            baml_type::typetag::LIST => "_[]".to_string(),
            baml_type::typetag::MAP => "map<_, _>".to_string(),
            other => format!("type tag {other}"),
        };
        self.set_operand(inst, OperandMeta::Const(meta));
        Ok(())
    }

    fn load_type(&mut self, template: &TyTemplate) -> Result<(), Self::Error> {
        let anchored = self.refs.anchor_template(template);
        let const_idx = self.add_constant(ConstValue::Type(anchored));
        let inst = self.emit(Instruction::LoadType(const_idx));
        self.set_operand(inst, OperandMeta::Const(self.spelled_template(template)));
        Ok(())
    }

    fn load_current_package(&mut self, package: &str) -> Result<(), Self::Error> {
        // The package is this unit's own; whoever lays the unit into an
        // executable writes its ordinal here. The spelling is display only.
        let inst = self.emit(Instruction::LoadCurrentPackage(0));
        self.set_operand(inst, OperandMeta::Const(package.to_string()));
        Ok(())
    }

    fn make_closure(&mut self, lambda_idx: usize, capture_count: usize) -> Result<(), Self::Error> {
        self.emit_make_closure_bytecode(lambda_idx, capture_count, 0);
        Ok(())
    }

    fn make_closure_with_type_args(
        &mut self,
        lambda_idx: usize,
        capture_count: usize,
        ntypeargs: usize,
    ) -> Result<(), Self::Error> {
        self.emit_make_closure_bytecode(lambda_idx, capture_count, ntypeargs);
        Ok(())
    }

    fn make_generic_function(
        &mut self,
        func: FunctionRef<'ctx>,
        ntypeargs: usize,
    ) -> Result<(), Self::Error> {
        let func_name = baml_compiler2_mir::function_link_name(self.db, func);
        let global_idx = self.function_global_index(func, "MakeGenericFunction: global not found");
        let ntypeargs = u16::try_from(ntypeargs).expect("ntypeargs fits u16");
        let inst = self.emit(Instruction::MakeGenericFunction {
            function: GlobalIndex::from_raw(global_idx),
            ntypeargs,
        });
        self.set_operand(inst, OperandMeta::Global(func_name));
        Ok(())
    }

    fn make_generic_function_from_value(&mut self, ntypeargs: usize) -> Result<(), Self::Error> {
        // The callable value and `ntypeargs` `Object::Type` values are already
        // on the stack (pushed by `walk_rvalue_pull`); just emit the opcode.
        let ntypeargs = u16::try_from(ntypeargs).expect("ntypeargs fits u16");
        self.emit(Instruction::MakeGenericFunctionFromValue { ntypeargs });
        Ok(())
    }

    fn load_deref_local(&mut self, local: Local) -> Result<(), Self::Error> {
        debug_assert!(
            self.captured_locals.contains(&local),
            "deref of uncaptured {local}"
        );
        self.emit(Instruction::LoadDeref(self.local_slots[&local]));
        Ok(())
    }

    fn load_capture_value(&mut self, idx: usize) -> Result<(), Self::Error> {
        self.emit(Instruction::LoadCapture(idx));
        Ok(())
    }

    fn load_capture_ref(&mut self, idx: usize) -> Result<(), Self::Error> {
        self.emit(Instruction::CaptureRef(idx));
        Ok(())
    }

    fn resolve_field_name(&self, base: &Place, field_idx: usize) -> String {
        let class = match self.resolve_place_type(base) {
            Some(RuntimeTy::Class(tn, _)) => self.refs.class_ref(&tn),
            _ => None,
        };
        class
            .and_then(|class| self.lookup_class_field_name(class, field_idx))
            .unwrap_or_else(|| format!("{field_idx}"))
    }

    fn class_field_name(&self, class: ClassRef<'ctx>, field_idx: usize) -> String {
        self.lookup_class_field_name(class, field_idx)
            .unwrap_or_else(|| format!("{field_idx}"))
    }
}

impl<'db: 'ctx, 'ctx> StackifyCodegen<'db, 'ctx, '_, '_> {
    /// `AllocInstance` for `class`: the `PullSink::alloc_class_instance`
    /// implementation above and the field-copy-set route share it.
    fn alloc_instance_of(&mut self, class: ClassRef<'ctx>, ntypeargs: u16) {
        let class_name = &baml_compiler2_mir::class_link_name(self.db, class);
        let class_obj_idx = self.refs.class(class);
        let inst = self.emit(Instruction::AllocInstance {
            class_obj: class_obj_idx,
            ntypeargs,
        });
        self.set_operand(inst, OperandMeta::Object(class_name.clone()));
    }
}

impl<'db: 'ctx, 'ctx> StackEffectSink<'ctx> for StackifyCodegen<'db, 'ctx, '_, '_> {
    fn store_field_value(&mut self, field: usize, name: &str) -> Result<(), Self::Error> {
        let idx = self.emit(Instruction::StoreField(field));
        self.set_operand(idx, OperandMeta::Field(name.to_string()));
        Ok(())
    }

    fn store_index_value(&mut self, kind: IndexKind) -> Result<(), Self::Error> {
        match kind {
            IndexKind::Array => self.emit(Instruction::StoreArrayElement),
            IndexKind::Map => self.emit(Instruction::StoreMapElement),
        };
        Ok(())
    }

    fn pop_values(&mut self, n: usize) -> Result<(), Self::Error> {
        self.emit(Instruction::Pop(n));
        Ok(())
    }
}

/// The coarse `IsType` type tag for a realized leaf type, or `None` for a type
/// with no representable tag (classes take the pointer-identity path instead).
fn realized_type_tag(ty: &RealizedTy) -> Option<i64> {
    match ty {
        RealizedTy::Int => Some(baml_type::typetag::INT),
        RealizedTy::Bigint => Some(baml_type::typetag::BIGINT),
        RealizedTy::String => Some(baml_type::typetag::STRING),
        RealizedTy::Bool => Some(baml_type::typetag::BOOL),
        RealizedTy::Null => Some(baml_type::typetag::NULL),
        RealizedTy::Float => Some(baml_type::typetag::FLOAT),
        RealizedTy::Enum(..) => Some(baml_type::typetag::ENUM),
        RealizedTy::List(..) => Some(baml_type::typetag::LIST),
        RealizedTy::Map { .. } => Some(baml_type::typetag::MAP),
        RealizedTy::Function { .. } => Some(baml_type::typetag::FUNCTION),
        RealizedTy::Type => Some(baml_type::typetag::TYPE),
        RealizedTy::Uint8Array => Some(baml_type::typetag::UINT8ARRAY),
        // A literal type has no type tag. Tags name base types, and a literal
        // is a strict subset of its base, so its base's tag over-accepts every
        // other inhabitant — `1` would admit any int. Literal membership is
        // decided by `ConstValue::Literal` in `is_type` above, which never
        // reaches here; returning `None` keeps that the only answer rather than
        // leaving a wrong one for the next caller to find.
        RealizedTy::Literal(..) => None,
        RealizedTy::Media(..)
        | RealizedTy::Class(..)
        | RealizedTy::Interface(..)
        | RealizedTy::Union(..)
        | RealizedTy::Future(..)
        | RealizedTy::RustType
        | RealizedTy::Resource
        | RealizedTy::PromptAst
        | RealizedTy::Void
        | RealizedTy::TypeAlias(..)
        | RealizedTy::Unknown
        | RealizedTy::Never
        | RealizedTy::EnumVariant(..) => None,
    }
}

// ============================================================================
// Public Entry Point
// ============================================================================

/// Compile a MIR function body to bytecode using stackification.
///
/// This is the main entry point for the optimized MIR-based code generation.
/// The caller is responsible for filling in `Function::name` after this returns.
/// If `mir_span` is provided, it is used to set `Function::span`.
pub(crate) fn compile_mir_function<'db: 'mir, 'mir>(
    body: &'mir MirFunctionBody,
    arity: usize,
    mir_span: Option<baml_base::Span>,
    line_starts: &'mir [u32],
    ctx: MirCodegenContext<'db, 'mir, '_, '_>,
    opt: crate::analysis::OptLevel,
) -> Function {
    // Run analysis
    let analysis = AnalysisResult::analyze(body, arity, opt);
    #[cfg(debug_assertions)]
    crate::verifier::verify_mir_emit_invariants(body, arity, &analysis);

    // Compile with stackification
    let codegen = StackifyCodegen::new(body, arity, line_starts, ctx, analysis);
    let mut f = codegen.compile();
    if let Some(span) = mir_span {
        f.span = span;
    }
    f
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use baml_compiler2_mir::{
        BasicBlock, BlockId, Constant, Local, LocalDecl, MirFunctionBody, Operand, Place,
        RuntimeTy, Rvalue, Statement, StatementKind, Terminator,
    };
    use bex_vm_types::{Instruction, ObjectPool};

    use super::compile_mir_function;
    use crate::{MirCodegenContext, analysis::OptLevel};

    fn local(ty: RuntimeTy) -> LocalDecl {
        LocalDecl {
            name: None,
            ty,
            span: None,
            scope_span: None,
            is_captured: false,
        }
    }

    #[test]
    fn branch_condition_is_emitted_even_when_else_is_unreachable() {
        let mut entry = BasicBlock::new(BlockId(0));
        entry.terminator = Some(Terminator::Branch {
            condition: Operand::copy_local(Local(1)),
            then_block: BlockId(1),
            else_block: BlockId(2),
        });

        let mut then_block = BasicBlock::new(BlockId(1));
        then_block.statements.push(Statement {
            kind: StatementKind::Assign {
                destination: Place::local(Local(0)),
                value: Rvalue::Use(Operand::constant(Constant::Int(1))),
            },
            span: None,
        });
        then_block.terminator = Some(Terminator::Goto { target: BlockId(3) });

        let mut unreachable_else = BasicBlock::new(BlockId(2));
        unreachable_else.terminator = Some(Terminator::Unreachable);

        let mut return_block = BasicBlock::new(BlockId(3));
        return_block.terminator = Some(Terminator::Return);

        let body = MirFunctionBody {
            blocks: vec![entry, then_block, unreachable_else, return_block],
            entry: BlockId(0),
            locals: vec![local(RuntimeTy::int()), local(RuntimeTy::bool())],
        };

        let class_fields = HashMap::new();
        let mut objects = ObjectPool::default();
        let lambda_object_indices = Vec::new();
        let lambda_names = Vec::new();
        let capture_types = Vec::new();
        let spawn_capture_indices = HashSet::new();
        let line_starts = [0];

        let db = crate::tests::TestDb::default();
        let mut refs = crate::refs::PackageRefs::new(&db, db.workspace(), crate::refs::Own::Tail);
        let function = compile_mir_function(
            &body,
            1,
            None,
            &line_starts,
            MirCodegenContext {
                db: &db,
                refs: &mut refs,
                class_fields: &class_fields,
                objects: &mut objects,
                objects_base: 0,
                lambda_object_indices: &lambda_object_indices,
                lambda_names: &lambda_names,
                capture_types: &capture_types,
                spawn_capture_indices: &spawn_capture_indices,
            },
            OptLevel::One,
        );

        assert!(
            function
                .bytecode
                .instructions
                .windows(2)
                .any(|window| matches!(
                    window,
                    [Instruction::LoadVar(1), Instruction::PopJumpIfFalse(_)]
                )),
            "expected branch bytecode to load the condition before PopJumpIfFalse, got: {:?}",
            function.bytecode.instructions
        );
    }

    /// Pin `switch_discriminant_pulls` to each strategy's emitted pull count —
    /// the contract the stack-carry simulation rejects candidates against. A
    /// drift here (a strategy pulling more or less than reported) recreates
    /// the stray-pop miscompile: pulls 2..N of an if-else chain popping
    /// unrelated stack slots under a stack-carried discriminant.
    #[test]
    fn switch_discriminant_pull_counts_per_strategy() {
        use super::{MirSwitchKey, switch_discriminant_pulls};
        let arms = |values: &[i64]| -> Vec<(MirSwitchKey<'static>, BlockId)> {
            values
                .iter()
                .map(|&v| (MirSwitchKey::Int(v), BlockId(0)))
                .collect()
        };

        // If-else chain (< 4 arms): one pull per emitted comparison; the
        // exhaustive final arm is elided, and its no-comparison forms (no
        // arms; a single exhaustive arm) pull zero times.
        assert_eq!(switch_discriminant_pulls(&arms(&[0, 1, 2]), false), 3);
        assert_eq!(switch_discriminant_pulls(&arms(&[0, 1, 2]), true), 2);
        assert_eq!(switch_discriminant_pulls(&arms(&[0, 1]), true), 1);
        assert_eq!(switch_discriminant_pulls(&arms(&[0]), true), 0);
        assert_eq!(switch_discriminant_pulls(&arms(&[]), false), 0);
        assert_eq!(switch_discriminant_pulls(&arms(&[]), true), 0);

        // Dense 4+ arms: jump table, single pull.
        assert_eq!(switch_discriminant_pulls(&arms(&[0, 1, 2, 3]), false), 1);
        // Sparse 4+ arms: switch table, single pull.
        assert_eq!(
            switch_discriminant_pulls(&arms(&[10, 2000, 300_000, 40_000_000]), false),
            1
        );
    }
}
