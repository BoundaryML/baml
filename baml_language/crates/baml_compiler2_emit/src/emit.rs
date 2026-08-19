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
use baml_compiler2_mir::{
    BasicBlock, BinOp, BlockId, Constant, IndexKind, IntrinsicOp, Local, LogLevel, MirFunctionBody,
    Operand, Place, Rvalue, StatementKind, Terminator, UnaryOp,
};
use baml_type::{RealizedTy, RuntimeTy, TyTemplate, TypeName};
use bex_vm_types::{
    BinOp as VmBinOp, Bytecode, CmpOp, ConstValue, Function, FunctionCaptureProps, FunctionKind,
    FunctionOrigin, GlobalIndex, Instruction, Object, ObjectIndex, ObjectPool,
    UnaryOp as VmUnaryOp,
    bytecode::{
        ClassInitPlan, DebugLocalScope, FieldCopy, FieldCopySet, InstructionMeta, JumpTableData,
        LineTableEntry, MatchHashEntry, MatchHashTable, OperandMeta,
    },
};

/// Coarse arithmetic-type classification used by [`try_specialize_binary_op`].
///
/// Collapses `RuntimeTy::Int { .. }` / `RuntimeTy::Literal(Int(_))` (and similar) into a
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
    /// Use binary search tree (O(log n) comparisons) for sparse integers.
    BinarySearch,
    /// Use perfect hash + dense jump table (O(1) dispatch) for sparse ≥4-arm switches.
    /// Replaces `BinarySearch` when a compile-time perfect hash is found.
    PerfectHash(PerfectHashResult),
    /// Use linear if-else chain (O(n) comparisons).
    IfElseChain,
}

// Tunable thresholds for switch emission strategy
const JUMP_TABLE_MIN_ARMS: usize = 4; // Minimum arms to consider jump table
const JUMP_TABLE_MIN_DENSITY: f64 = 0.5; // Minimum density for jump table
const JUMP_TABLE_MAX_SIZE: usize = 256; // Maximum jump table size
const BINARY_SEARCH_MIN_ARMS: usize = 4; // Minimum arms for binary search

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
fn analyze_switch(arms: &[(i64, BlockId)]) -> SwitchStrategy {
    // No arms - use if-else (will just jump to otherwise)
    if arms.is_empty() {
        return SwitchStrategy::IfElseChain;
    }

    // Find min and max values
    let min = arms.iter().map(|(v, _)| *v).min().unwrap();
    let max = arms.iter().map(|(v, _)| *v).max().unwrap();
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
    // Use binary search for sparse but large switch, with perfect hash
    // preferred when a compile-time hash is found (O(1) vs O(log K)).
    //
    // Perfect hashing is always attempted here. It's only reached when
    // density is too low for JumpTable, so it won't interfere with dense
    // enum discriminant switches. See `find_perfect_hash` for algorithm
    // details and references.
    else if arms.len() >= BINARY_SEARCH_MIN_ARMS {
        let keys: Vec<(i64, usize)> = arms.iter().enumerate().map(|(i, (v, _))| (*v, i)).collect();
        if let Some(result) = find_perfect_hash(&keys) {
            SwitchStrategy::PerfectHash(result)
        } else {
            SwitchStrategy::BinarySearch
        }
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
/// - `JumpTable` / `PerfectHash` / `BinarySearch` pull exactly once
///   (`BinarySearch` keeps the value on the stack via `Copy` thereafter);
/// - `IfElseChain` re-loads the discriminant once per emitted comparison —
///   `arms.len()` minus the exhaustive-final elision — including ZERO pulls
///   for its no-comparison forms (no arms; a single exhaustive arm).
///
/// A stack-carried discriminant is only sound at exactly one pull: the carried
/// value is consumed by the first pull, so later pulls would pop unrelated
/// stack slots and a zero-pull form would orphan it (see `stack_carry`'s
/// `Terminator::Switch` arm, the sole consumer).
pub(crate) fn switch_discriminant_pulls(arms: &[(i64, BlockId)], exhaustive: bool) -> usize {
    match analyze_switch(arms) {
        SwitchStrategy::JumpTable { .. }
        | SwitchStrategy::PerfectHash(_)
        | SwitchStrategy::BinarySearch => 1,
        SwitchStrategy::IfElseChain => arms.len().saturating_sub(usize::from(exhaustive)),
    }
}

/// Result of a successful perfect hash search.
#[derive(Debug)]
struct PerfectHashResult {
    multiply: u64,
    shift: u8,
    mask: u8,
    /// Verification + dispatch entries indexed by hash slot.
    entries: Vec<MatchHashEntry>,
}

/// Find a minimal perfect hash for a small set of integer keys.
///
/// Searches for constants `(M, S, mask)` such that
///   `h(x) = ((x as u64).wrapping_mul(M) >> S) & mask`
/// maps all keys to distinct slots in `[0, table_size)`.
///
/// The search tries increasing table sizes (`next_power_of_two(K)`, then 2x)
/// and brute-forces M values with all 64 shift values. For K ≤ 20 this
/// completes in microseconds.
///
/// Returns `None` if no perfect hash is found (practically impossible for
/// K ≤ 20, but handled for safety).
///
/// # Algorithm
///
/// This implements the multiply-shift hash family described in:
/// - Neumann & Göbbert, "Improving Switch Statement Performance with Hashing
///   Optimized at Compile Time"
/// - Dietz 1992, "Coding Multiway Branches Using Customized Hash Functions"
///
/// The approach has been proposed for production compilers: LLVM issue #96971,
/// Roslyn #66604, Go #34381.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]
fn find_perfect_hash(keys: &[(i64, usize)]) -> Option<PerfectHashResult> {
    let k = keys.len();
    // Cap at 128: `MatchHashEntry::dense_index` is a `u8` (max 255), and
    // the birthday paradox makes collision-free hashing increasingly unlikely
    // past ~128 keys in a 256-slot table. Beyond this threshold the brute-force
    // search would waste ~20-40ms of compile time before giving up. Falls back
    // to binary search (O(log K)) which is fine at this scale.
    if k == 0 || k > 128 {
        return None;
    }

    // Try table sizes: tight (next power of 2), then 2x for easier search.
    for table_size in [k.next_power_of_two(), (k.next_power_of_two() * 2).min(256)] {
        let mask = (table_size - 1) as u8;
        let mut slots = vec![false; table_size];

        for m in 1u64..10_000 {
            for s in 0u8..64 {
                // Test if (m, s) produces distinct hashes for all keys.
                slots.fill(false);
                let mut ok = true;
                for &(key, _) in keys {
                    let h = ((key as u64).wrapping_mul(m) >> s) & mask as u64;
                    let h = h as usize;
                    if h >= table_size || slots[h] {
                        ok = false;
                        break;
                    }
                    slots[h] = true;
                }
                if ok {
                    // Build the verification + dispatch table.
                    let mut entries = vec![
                        MatchHashEntry {
                            expected_tag: i64::MIN, // sentinel for empty slots
                            dense_index: 0,
                        };
                        table_size
                    ];
                    for (dense_idx, &(key, _arm_idx)) in keys.iter().enumerate() {
                        let h = ((key as u64).wrapping_mul(m) >> s) & mask as u64;
                        entries[h as usize] = MatchHashEntry {
                            expected_tag: key,
                            dense_index: dense_idx as u8,
                        };
                    }
                    return Some(PerfectHashResult {
                        multiply: m,
                        shift: s,
                        mask,
                        entries,
                    });
                }
            }
        }
    }
    None
}

use crate::{
    MirCodegenContext,
    analysis::{AnalysisResult, LocalClassification, StatementRef},
    pull_semantics::{
        self, LocalAssignBehavior, LocalPullAction, LocalStoreBehavior, PullSink, StackEffectSink,
    },
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

#[derive(Default)]
struct SpawnCaptures {
    locals: HashSet<Local>,
    capture_indices: HashSet<usize>,
}

/// MIR to bytecode compiler with stackification.
struct StackifyCodegen<'ctx, 'obj> {
    /// MIR body being compiled.
    body: &'ctx MirFunctionBody,
    /// Arity (parameter count) of the function being compiled.
    arity: usize,
    /// Line index for the MIR's source file.
    line_starts: &'ctx [u32],

    /// Resolved global names to indices.
    globals: &'ctx HashMap<String, usize>,
    /// Resolved class field indices.
    #[allow(dead_code)]
    classes: &'ctx HashMap<String, HashMap<String, usize>>,
    /// Pre-allocated Class object indices.
    class_object_indices: &'ctx HashMap<String, usize>,
    /// Pre-allocated Enum object indices.
    enum_object_indices: &'ctx HashMap<String, usize>,
    /// Enum variant mappings (enum name -> variant name -> variant index).
    enum_variants: &'ctx HashMap<String, HashMap<String, usize>>,
    /// Read-only snapshot of pooled class field metadata (name + type, in
    /// field order), keyed by every name registered in `class_object_indices`.
    /// Field lookups resolve through this map instead of reading the object
    /// pool, so codegen never reads pool contents (parallel emit compiles
    /// against fragment pools that don't contain the pre-existing objects).
    class_fields: &'ctx crate::ClassFieldSnapshot,
    /// Object pool this function's codegen mints into. Serial emit passes the
    /// whole program pool; parallel emit passes a worker-local fragment pool.
    objects: &'obj mut ObjectPool,
    /// Program-absolute index of `objects[0]`. Serial emit mints into the
    /// program pool directly (base 0); parallel workers mint into a fresh
    /// fragment pool based at the shared watermark, so every index this
    /// codegen embeds is program-absolute either way.
    objects_base: usize,

    /// Analysis results (classifications, def-use, etc.).
    analysis: AnalysisResult,

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

    /// MIR local types for field name resolution (debug info).
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

    /// When `true`, the current operand load is for a `MakeClosure` capture operand.
    /// In that case, captured locals are loaded with `LoadVar` (to pass the cell
    /// pointer itself) rather than `LoadDeref` (which would dereference the cell).
    loading_for_closure_capture: bool,
}

impl<'ctx, 'obj> StackifyCodegen<'ctx, 'obj> {
    fn display_string_operand(value: &str) -> String {
        format!("{value:?}")
    }

    /// Create a new stackification codegen instance.
    #[allow(clippy::needless_pass_by_value)] // ctx is destructured into self fields
    fn new(
        body: &'ctx MirFunctionBody,
        arity: usize,
        line_starts: &'ctx [u32],
        ctx: MirCodegenContext<'ctx, 'obj>,
        analysis: AnalysisResult,
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
            globals: ctx.globals,
            classes: ctx.classes,
            class_object_indices: ctx.class_object_indices,
            enum_object_indices: ctx.enum_object_indices,
            enum_variants: ctx.enum_variants,
            class_fields: ctx.class_fields,
            objects: ctx.objects,
            objects_base: ctx.objects_base,
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
            loading_for_closure_capture: false,
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

    /// Look up a field name from the class-field snapshot given a class name
    /// and field index.
    fn lookup_class_field_name(&self, class_name: &str, field_idx: usize) -> Option<String> {
        self.class_fields
            .get(class_name)?
            .get(field_idx)
            .map(|(name, _)| name.clone())
    }

    fn class_object_index_for_type_name(&self, tn: &TypeName) -> Option<usize> {
        let full_name = tn.render_dotted(false);
        self.class_object_indices
            .get(&full_name)
            .copied()
            .or_else(|| {
                self.class_object_indices
                    .get(tn.display_name().as_str())
                    .copied()
            })
            .or_else(|| self.class_object_indices.get(tn.name().as_str()).copied())
    }

    /// Class field metadata for a class type name, resolved through the same
    /// name fallbacks as [`Self::class_object_index_for_type_name`] but
    /// against the read-only snapshot instead of the pool.
    fn class_fields_for_type_name(&self, tn: &TypeName) -> Option<&[(String, RuntimeTy)]> {
        let full_name = tn.render_dotted(false);
        self.class_fields
            .get(&full_name)
            .or_else(|| self.class_fields.get(tn.display_name().as_str()))
            .or_else(|| self.class_fields.get(tn.name().as_str()))
            .map(Vec::as_slice)
    }

    /// Enum-object index for an enum type name, mirroring
    /// [`Self::class_object_index_for_type_name`]. Used by `is <Enum>` to test
    /// enum identity (`ConstValue::Object`) rather than the shared `ENUM` tag,
    /// which cannot distinguish two enum types (`Color` vs `Status`).
    fn enum_object_index_for_type_name(&self, tn: &TypeName) -> Option<usize> {
        let full_name = tn.render_dotted(false);
        self.enum_object_indices
            .get(&full_name)
            .copied()
            .or_else(|| {
                self.enum_object_indices
                    .get(tn.display_name().as_str())
                    .copied()
            })
            .or_else(|| self.enum_object_indices.get(tn.name().as_str()).copied())
    }

    /// Resolve the type of a MIR Place by walking from the root local through projections.
    fn resolve_place_type(&self, place: &Place) -> Option<RuntimeTy> {
        match place {
            Place::Local(local) => self.local_types.get(local).cloned(),
            Place::Capture(idx) => self.capture_types.get(*idx).cloned(),
            Place::Field { base, field } => {
                let base_ty = self.resolve_place_type(base)?;
                match &base_ty {
                    RuntimeTy::Class(type_name, _, _) => self
                        .class_fields_for_type_name(type_name)?
                        .get(*field)
                        .map(|(_, field_type)| field_type.clone()),
                    _ => None,
                }
            }
            Place::Index { base, .. } => {
                let base_ty = self.resolve_place_type(base)?;
                match base_ty {
                    RuntimeTy::List(inner, _) => Some(*inner),
                    RuntimeTy::Map { value, .. } => Some(*value),
                    _ => None,
                }
            }
        }
    }

    /// Resolve the compile-time type of an operand, if known.
    fn resolve_operand_type(&self, operand: &Operand) -> Option<RuntimeTy> {
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
    /// Both `RuntimeTy::Int { .. }` and `RuntimeTy::Literal(Literal::Int(_), _)` map to
    /// `Int`, and similarly for `Float`/`Bigint`. This lets us specialize
    /// expressions like `(-1n) & 255n` where the lhs operand carries a
    /// `RuntimeTy::Literal(Bigint(-1))` after constant-folding in TIR.
    fn classify_arith_ty(ty: &RuntimeTy) -> Option<ArithTyClass> {
        match ty {
            RuntimeTy::Int { .. } => Some(ArithTyClass::Int),
            RuntimeTy::Float { .. } => Some(ArithTyClass::Float),
            RuntimeTy::Bigint { .. } => Some(ArithTyClass::Bigint),
            RuntimeTy::Literal(baml_type::Literal::Int(_), _, _) => Some(ArithTyClass::Int),
            RuntimeTy::Literal(baml_type::Literal::Float(_), _, _) => Some(ArithTyClass::Float),
            RuntimeTy::Literal(baml_type::Literal::Bigint(_), _, _) => Some(ArithTyClass::Bigint),
            _ => None,
        }
    }

    fn collect_spawn_captures(&self) -> SpawnCaptures {
        let mut captures = SpawnCaptures::default();
        let mut seen = HashSet::new();

        for block in &self.body.blocks {
            let Some(Terminator::Spawn { closure, .. }) = &block.terminator else {
                continue;
            };

            self.collect_spawn_closure_captures(closure, &mut captures, &mut seen);
        }

        captures
    }

    fn collect_spawn_closure_captures(
        &self,
        operand: &Operand,
        captures: &mut SpawnCaptures,
        seen: &mut HashSet<Local>,
    ) {
        if let Some(Rvalue::MakeClosure {
            captures: closure_captures,
            ..
        }) = self.local_def_rvalue_for_operand(operand)
        {
            for capture in closure_captures {
                self.collect_spawn_shared_operand(capture, captures, seen);
            }
            return;
        }

        self.collect_spawn_shared_operand(operand, captures, seen);
    }

    fn collect_spawn_shared_operand(
        &self,
        operand: &Operand,
        captures: &mut SpawnCaptures,
        seen: &mut HashSet<Local>,
    ) {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => {
                self.collect_spawn_shared_place(place, captures, seen);
            }
            Operand::Constant(_) => {}
        }
    }

    fn collect_spawn_shared_place(
        &self,
        place: &Place,
        captures: &mut SpawnCaptures,
        seen: &mut HashSet<Local>,
    ) {
        match place {
            Place::Local(local) => self.collect_spawn_shared_local(*local, captures, seen),
            Place::Capture(idx) => {
                captures.capture_indices.insert(*idx);
            }
            Place::Field { base, .. } => self.collect_spawn_shared_place(base, captures, seen),
            Place::Index { base, index, .. } => {
                self.collect_spawn_shared_place(base, captures, seen);
                self.collect_spawn_shared_local(*index, captures, seen);
            }
        }
    }

    fn collect_spawn_shared_local(
        &self,
        local: Local,
        captures: &mut SpawnCaptures,
        seen: &mut HashSet<Local>,
    ) {
        let local = match self.analysis.classifications.get(&local).copied() {
            Some(LocalClassification::CopyOf) => self.analysis.resolve_copy_source(local),
            _ => local,
        };

        if !seen.insert(local) {
            return;
        }

        if self.local_slots.contains_key(&local) {
            captures.locals.insert(local);
        }

        match self.local_def_rvalue(local) {
            Some(Rvalue::MakeClosure {
                captures: closure_captures,
                ..
            }) => {
                for capture in closure_captures {
                    self.collect_spawn_shared_operand(capture, captures, seen);
                }
            }
            Some(Rvalue::Use(operand)) => {
                self.collect_spawn_shared_operand(operand, captures, seen);
            }
            Some(Rvalue::MakeBoundMethod { receiver, .. }) => {
                self.collect_spawn_shared_operand(receiver, captures, seen);
            }
            _ => {}
        }
    }

    fn local_def_rvalue(&self, local: Local) -> Option<&Rvalue> {
        self.analysis
            .def_use
            .get(&local)
            .and_then(|du| du.def.as_ref())
            .map(|def| &def.rvalue)
    }

    fn local_def_rvalue_for_operand(&self, operand: &Operand) -> Option<&Rvalue> {
        let place = match operand {
            Operand::Copy(place) | Operand::Move(place) => place,
            Operand::Constant(_) => return None,
        };

        let Place::Local(local) = place else {
            return None;
        };

        let local = match self.analysis.classifications.get(local).copied() {
            Some(LocalClassification::CopyOf) => self.analysis.resolve_copy_source(*local),
            _ => *local,
        };

        self.local_def_rvalue(local)
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
            Place::Capture(idx) => self.spawn_captured_captures.contains(idx),
            Place::Field { base, .. } => self.place_reads_spawn_captured_local(base, seen),
            Place::Index { base, index, .. } => {
                self.place_reads_spawn_captured_local(base, seen)
                    || self.local_reads_spawn_captured_local(*index, seen)
            }
        }
    }

    fn operand_reads_spawn_captured_local(
        &self,
        operand: &Operand,
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
        rvalue: &Rvalue,
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
            Rvalue::RuntimeIsType {
                operand,
                type_value,
            } => {
                self.operand_reads_spawn_captured_local(operand, seen)
                    || self.operand_reads_spawn_captured_local(type_value, seen)
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

    fn binary_operands_can_use_specialized_op(&self, left: &Operand, right: &Operand) -> bool {
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
        left: &Operand,
        right: &Operand,
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
        let spawn_captures = self.collect_spawn_captures();
        self.spawn_captured_locals = spawn_captures.locals;
        self.spawn_captured_captures
            .extend(spawn_captures.capture_indices);

        // Emit cell-wrapping preamble: for each captured Real local, wrap the
        // initial value in a Cell so that lambdas can share and mutate it.
        // Emit at the start of the entry block before any user instructions.
        // Note: Parameters that are captured also need cell wrapping.
        for (i, local_decl) in mir.locals.iter().enumerate() {
            if local_decl.is_captured {
                let local = Local(i);
                if let Some(&slot) = self.local_slots.get(&local) {
                    // Load the current value (either 0 for uninitialized or param value),
                    // wrap in a Cell, and store back.
                    let inst = self.emit(Instruction::LoadVar(slot));
                    self.set_var_operand(inst, slot);
                    self.emit(Instruction::MakeCell);
                    let inst = self.emit(Instruction::StoreVar(slot));
                    self.set_var_operand(inst, slot);
                }
            }
        }

        // Build local type map for field name resolution (debug info).
        for (i, local_decl) in mir.locals.iter().enumerate() {
            self.local_types.insert(Local(i), local_decl.ty.clone());
        }

        // Build slot name mapping for debug metadata.
        self.slot_names = Self::build_local_names(mir, &self.local_slots);

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

        // 4. Build exception table from MIR catch regions
        self.build_exception_table(mir);

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
            local_names: self.slot_names,
            debug_locals,
            span: Span::fake(),
            return_type: baml_type::TyTemplate::Null {
                attr: baml_type::TyAttr::default(),
            },
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            generic_param_bounds: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type: "null".to_string(),
            throws_type: baml_type::TyTemplate::Never {
                attr: baml_type::TyAttr::default(),
            },
            origin: FunctionOrigin::Internal,
            body_meta: None,
            capture: FunctionCaptureProps::disabled(),
            function_id: 0, // assigned at engine init (interim provider)
            runtime_package: bex_vm_types::HeapPtr::null(),
        }
    }

    /// Allocate stack slots only for Real locals.
    ///
    /// Virtual locals don't get slots - they're inlined at use sites.
    fn allocate_real_locals(&mut self, mir: &MirFunctionBody) {
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
    fn emit_block(&mut self, block: &BasicBlock) {
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
    fn emit_statement(&mut self, kind: &StatementKind) {
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
                    Place::Local(_) => {
                        // Local assignment: emit rvalue then store
                        self.emit_rvalue_pull(value);
                        self.emit_store_place(destination);
                    }
                    Place::Capture(idx) => {
                        // Capture store: evaluate rvalue, then StoreCapture.
                        self.emit_rvalue_pull(value);
                        unwrap_infallible(self.store_capture_value(*idx));
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
                let iface_const = self.add_constant(ConstValue::Type(iface.to_template()));
                let inst = self.emit(Instruction::LoadType(iface_const));
                self.set_operand(inst, OperandMeta::Const(iface.to_string()));
                let inst = self.emit(Instruction::VirtualStoreField(*field_index as usize));
                self.set_operand(inst, OperandMeta::Field(field.to_string()));
            }
            StatementKind::Drop(place) => {
                unwrap_infallible(pull_semantics::walk_drop_statement(self, place));
            }
            StatementKind::VizEnter(_node_idx) => {
                // Viz observability is not emitted to bytecode.
            }
            StatementKind::VizExit(_node_idx) => {
                // Viz observability is not emitted to bytecode.
            }
            StatementKind::FreshCell(local) => {
                if self.captured_locals.contains(local) {
                    if let Some(&slot) = self.local_slots.get(local) {
                        let null_idx = self.add_constant(ConstValue::Null);
                        let inst = self.emit(Instruction::LoadConst(null_idx));
                        self.set_operand(inst, OperandMeta::Const("null".to_string()));
                        self.emit(Instruction::MakeCell);
                        let inst = self.emit(Instruction::StoreVar(slot));
                        self.set_var_operand(inst, slot);
                    }
                }
            }
            StatementKind::Intrinsic { op, args } => {
                match op {
                    IntrinsicOp::BindType(slot) => {
                        let [value] = args.as_slice() else {
                            panic!("BindType expects exactly one operand")
                        };
                        self.emit_operand_pull(value);
                        self.emit(Instruction::BindType(*slot));
                    }
                    IntrinsicOp::Log(level) => {
                        // Emit the reserved "$baml_log" event with payload
                        // { level: "<level>", data: <user_arg> }, where
                        // <user_arg> may be any BAML value.

                        // Save call-site span — walking args may overwrite current_debug_span
                        let call_site_span = self.current_debug_span;

                        // 1. Push event name "$baml_log"
                        let log_str_idx = self.mint_object(Object::String("$baml_log".into()));
                        let log_const_idx = self
                            .add_constant(ConstValue::Object(ObjectIndex::from_raw(log_str_idx)));
                        let inst = self.emit(Instruction::LoadConst(log_const_idx));
                        self.set_operand(
                            inst,
                            OperandMeta::Const(Self::display_string_operand("$baml_log")),
                        );

                        // 2. Push level value string
                        let level_str = match level {
                            LogLevel::Info => "info",
                            LogLevel::Debug => "debug",
                            LogLevel::Warn => "warn",
                            LogLevel::Error => "error",
                        };
                        let level_val_idx = self.mint_object(Object::String(level_str.into()));
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
                        let level_key_idx = self.mint_object(Object::String("level".into()));
                        let level_key_const_idx = self
                            .add_constant(ConstValue::Object(ObjectIndex::from_raw(level_key_idx)));
                        let inst = self.emit(Instruction::LoadConst(level_key_const_idx));
                        self.set_operand(
                            inst,
                            OperandMeta::Const(Self::display_string_operand("level")),
                        );

                        // 5. Push key "data"
                        let data_key_idx = self.mint_object(Object::String("data".into()));
                        let data_key_const_idx = self
                            .add_constant(ConstValue::Object(ObjectIndex::from_raw(data_key_idx)));
                        let inst = self.emit(Instruction::LoadConst(data_key_const_idx));
                        self.set_operand(
                            inst,
                            OperandMeta::Const(Self::display_string_operand("data")),
                        );

                        // 6. Push the payload map's key/value type tags, then
                        //    AllocMap(2) -> { level: "info", data: <user_data> }.
                        //    The event is a `map<string, unknown>` (string keys;
                        //    heterogeneous values). The VM's `AllocMap` pops the
                        //    value type (top of stack) then the key type (below it)
                        //    before draining the entries, so push key first, value
                        //    second — mirroring the `alloc_map` helper. Omitting
                        //    these tags makes the VM read the entry keys as types.
                        unwrap_infallible(self.load_type(&TyTemplate::from(RealizedTy::string())));
                        unwrap_infallible(self.load_type(&TyTemplate::from(RealizedTy::unknown())));
                        self.emit(Instruction::AllocMap(2));

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
    fn emit_operand_pull(&mut self, operand: &Operand) {
        unwrap_infallible(pull_semantics::walk_operand_pull(self, operand));
    }

    fn emit_init_spread(&mut self, fields: Vec<FieldCopy>, display_fields: &[String]) {
        let set_idx = self.bytecode.field_copy_sets.len();
        self.bytecode.field_copy_sets.push(FieldCopySet { fields });
        let inst = self.emit(Instruction::InitSpread(set_idx));
        self.set_operand(inst, OperandMeta::Field(display_fields.join(", ")));
    }

    fn emit_init_instance(&mut self, class_name: &str, ntypeargs: u16, field_count: usize) {
        if let Some(&class_obj_idx) = self.class_object_indices.get(class_name) {
            let fields = (0..field_count).collect::<Vec<_>>();
            let display_fields = fields
                .iter()
                .map(|field_idx| format!(".{}", self.class_field_name(class_name, *field_idx)))
                .collect::<Vec<_>>();
            let plan_idx = self.bytecode.class_init_plans.len();
            self.bytecode.class_init_plans.push(ClassInitPlan {
                class_obj: ObjectIndex::from_raw(class_obj_idx),
                ntypeargs,
                fields,
            });
            let inst = self.emit(Instruction::InitInstance(plan_idx));
            self.set_operand(
                inst,
                OperandMeta::Object(format!("{class_name} {}", display_fields.join(", "))),
            );
        } else {
            let total_inputs = field_count + usize::from(ntypeargs);
            if total_inputs > 0 {
                self.emit(Instruction::Pop(total_inputs));
            }
            let null_idx = self.add_constant(bex_vm_types::ConstValue::Null);
            let inst = self.emit(bex_vm_types::Instruction::LoadConst(null_idx));
            self.set_operand(
                inst,
                OperandMeta::Const(format!("null /* unknown class: {class_name} */")),
            );
        }
    }

    fn field_copy_operand(operand: &Operand) -> Option<(&Place, usize)> {
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
        class_name: &str,
        type_arg_templates: &[TyTemplate],
        fields: &[Operand],
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
        self.emit_init_instance(class_name, ntypeargs, fields.len());
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
            Place::Capture(_) => false,
        }
    }

    fn try_emit_class_aggregate_field_copy_sets(
        &mut self,
        class_name: &str,
        type_arg_templates: &[TyTemplate],
        fields: &[Operand],
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
        unwrap_infallible(self.alloc_class_instance(class_name, ntypeargs));

        let mut field_idx = 0usize;
        while field_idx < fields.len() {
            let Some((base, source_field)) = Self::field_copy_operand(&fields[field_idx]) else {
                let name = self.class_field_name(class_name, field_idx);
                self.emit_operand_pull(&fields[field_idx]);
                unwrap_infallible(self.init_field(field_idx, &name));
                field_idx += 1;
                continue;
            };

            if self.place_mentions_stack_carried_local(base) {
                let name = self.class_field_name(class_name, field_idx);
                self.emit_operand_pull(&fields[field_idx]);
                unwrap_infallible(self.init_field(field_idx, &name));
                field_idx += 1;
                continue;
            }

            let mut copies = vec![FieldCopy {
                source: source_field,
                dest: field_idx,
            }];
            let mut display_fields =
                vec![format!(".{}", self.class_field_name(class_name, field_idx))];
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
                display_fields.push(format!(".{}", self.class_field_name(class_name, field_idx)));
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
    fn emit_copy_aware_field_store(&mut self, destination: &Place, value: &Rvalue) -> bool {
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
        self.emit(Self::binop_instruction(*op));
        unwrap_infallible(self.store_field_value(*field, &name));
        true
    }

    /// Emit an rvalue using the pull model.
    fn emit_rvalue_pull(&mut self, rvalue: &Rvalue) {
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
            let prev = self.loading_for_closure_capture;
            self.loading_for_closure_capture = true;
            for capture in captures {
                self.emit_operand_pull(capture);
            }
            self.loading_for_closure_capture = prev;
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
                    name,
                    type_arg_templates,
                },
            fields,
        } = rvalue
        {
            if !self.class_object_indices.contains_key(name) {
                for field in fields {
                    self.emit_operand_pull(field);
                }
                let ntypeargs = u16::try_from(type_arg_templates.len())
                    .expect("type_arg_templates count fits in u16");
                for template in type_arg_templates {
                    unwrap_infallible(self.load_type(template));
                }
                self.emit_init_instance(name, ntypeargs, fields.len());
                return;
            }
            if self.try_emit_class_aggregate_init_instance(name, type_arg_templates, fields) {
                return;
            }
            if self.try_emit_class_aggregate_field_copy_sets(name, type_arg_templates, fields) {
                return;
            }
        }
        // Specialize BinaryOp when both operand types are statically known.
        if let Rvalue::BinaryOp { op, left, right } = rvalue {
            if let Some(specialized) = self.try_specialize_binary_op(*op, left, right) {
                self.emit_operand_pull(left);
                self.emit_operand_pull(right);
                self.emit(specialized);
                return;
            }
        }
        if let Rvalue::MakeBoundMethod { item_ref, receiver } = rvalue {
            // Emit the receiver onto the stack first.
            self.emit_operand_pull(receiver);
            // Resolve the item_ref to a GlobalIndex.
            let func_name = item_ref.to_string();
            let global_idx = *self
                .globals
                .get(&func_name)
                .unwrap_or_else(|| panic!("MakeBoundMethod: global not found for {func_name}"));
            let inst = self.emit(Instruction::MakeBoundMethod(GlobalIndex::from_raw(
                global_idx,
            )));
            self.set_operand(inst, OperandMeta::Global(func_name));
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
                let const_idx = self.add_constant(ConstValue::Type(template.clone()));
                let inst = self.emit(Instruction::LoadType(const_idx));
                self.set_operand(inst, OperandMeta::Const(template.to_string()));
            }
            let iface_const = self.add_constant(ConstValue::Type(iface.to_template()));
            let inst = self.emit(Instruction::LoadType(iface_const));
            self.set_operand(inst, OperandMeta::Const(iface.to_string()));
            self.emit_constant(&Constant::String(method.clone()));
            let inst = self.emit(Instruction::MakeVirtualBoundMethod {
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
            let iface_const = self.add_constant(ConstValue::Type(iface.to_template()));
            let inst = self.emit(Instruction::LoadType(iface_const));
            self.set_operand(inst, OperandMeta::Const(iface.to_string()));
            let inst = self.emit(Instruction::VirtualLoadField(*field_index as usize));
            self.set_operand(inst, OperandMeta::Field(field.to_string()));
            return;
        }
        // `MakeGenericFunction` needs no special handling here (it has no value
        // captures) — `walk_rvalue_pull` emits it uniformly for both the direct
        // and inlined paths.
        unwrap_infallible(pull_semantics::walk_rvalue_pull(self, rvalue));
    }

    /// Push a function reference as a value: a pooled, interned
    /// `Object::GenericFunction` wrapper over the function's global slot
    /// (empty `type_args` for a plain reference). Interning by
    /// (function, `type_args`) over the shared object pool makes identical
    /// references share ONE pooled object → pointer-stable identity
    /// (`greet === greet`, `foo<int> === foo<int>`).
    ///
    /// Serial emit scans the whole program pool here, so wrappers minted by
    /// EARLIER functions are reused too. Parallel emit scans only this
    /// worker's fragment; the serial merge replays the cross-function dedup
    /// in original function order (see `merge_function_fragment`),
    /// reproducing the exact serial candidate set and pool layout.
    fn emit_pooled_function_value(
        &mut self,
        item: &baml_compiler2_mir::ItemRef,
        type_args: &[baml_type::RealizedTy],
    ) {
        let name_str = item.to_string();
        let global_idx = *self
            .globals
            .get(&name_str)
            .unwrap_or_else(|| panic!("undefined function: {name_str}"));
        let gidx = GlobalIndex::from_raw(global_idx);
        let existing = self
            .objects
            .iter()
            .position(|o| {
                matches!(o, Object::GenericFunction(gf)
                if gf.function == gidx && gf.type_args.as_ref() == type_args)
            })
            .map(|local| self.objects_base + local);
        let pool_idx = match existing {
            Some(idx) => idx,
            None => self.mint_object(Object::GenericFunction(bex_vm_types::GenericFunction {
                function: gidx,
                type_args: type_args.to_vec().into_boxed_slice(),
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

    fn emit_constant(&mut self, constant: &Constant) {
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
                let obj_idx = self.mint_object(Object::String(s.as_str().into()));
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
            Constant::Function(item_ref) => {
                // A plain function reference as a VALUE. Pooled exactly like
                // `Constant::GenericFunction`, with EMPTY type args: every
                // function-pointer value on the heap is a wrapper object
                // (`GenericFunction`/`Closure`/`BoundMethod`/`HostClosure`),
                // and a raw `Object::Function` is never a data value — the
                // invariant `value_concrete_ty` / `callable_signature` rely
                // on. Interning keeps `greet === greet` pointer-stable, as a
                // direct `LoadGlobal` of the function object did before.
                self.emit_pooled_function_value(item_ref, &[]);
            }
            Constant::GlobalItem(item_ref) => {
                // A non-function global item (a client, a top-level `let`,
                // ...): read the value `$init` stored in its slot, unwrapped.
                let name_str = item_ref.to_string();
                let global_idx = self
                    .globals
                    .get(&name_str)
                    .unwrap_or_else(|| panic!("undefined global item: {name_str}"));
                let inst = self.emit(Instruction::LoadGlobal(GlobalIndex::from_raw(*global_idx)));
                self.set_operand(inst, OperandMeta::Global(name_str));
            }
            Constant::GenericFunction { item, type_args } => {
                // `foo<int>` as a value: the same pooled wrapper, carrying its
                // concrete type arguments so calling it seeds `frame.type_args`.
                self.emit_pooled_function_value(item, type_args);
            }
            Constant::EnumVariant { enum_ref, variant } => {
                let enum_name_str = enum_ref.to_string();
                // Gracefully handle undefined enum references (e.g. cross-package
                // references that aren't registered in this compilation context).
                // Emit a Null constant so tests don't panic; runtime will fail
                // if the code path is actually executed.
                let Some(enum_obj_idx) = self.enum_object_indices.get(&enum_name_str).copied()
                else {
                    let idx = self.add_constant(ConstValue::Null);
                    let inst = self.emit(Instruction::LoadConst(idx));
                    self.set_operand(
                        inst,
                        OperandMeta::Const(format!("undefined_enum::{enum_name_str}.{variant}")),
                    );
                    return;
                };

                let variant_str = variant.to_string();
                let variant_idx = *self
                    .enum_variants
                    .get(&enum_name_str)
                    .and_then(|variants| variants.get(&variant_str))
                    .unwrap_or_else(|| panic!("undefined variant: {enum_name_str}.{variant_str}"));

                #[allow(clippy::cast_possible_wrap)]
                let idx = self.add_constant(ConstValue::Int(variant_idx as i64));
                let lc_inst = self.emit(Instruction::LoadConst(idx));
                self.set_operand(
                    lc_inst,
                    OperandMeta::Const(format!("{enum_name_str}.{variant_str}")),
                );
                let inst = self.emit(Instruction::AllocVariant(ObjectIndex::from_raw(
                    enum_obj_idx,
                )));
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
                        let slot = self.local_slots[local];
                        if self.captured_locals.contains(local) {
                            // Captured local: store through the cell.
                            self.emit(Instruction::StoreDeref(slot));
                        } else {
                            // Normal local: direct slot store (folds a preceding
                            // StoreVar into StoreVar2).
                            self.emit_store_var(slot);
                        }
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
            Place::Capture(idx) => {
                // StoreCapture for lambda body capture stores.
                self.emit(Instruction::StoreCapture(*idx));
            }
            Place::Field { .. } | Place::Index { .. } => {
                unreachable!(
                    "Field/Index stores are handled in emit_statement, not emit_store_place"
                );
            }
        }
    }

    // ========================================================================
    // Terminator Emission
    // ========================================================================

    fn emit_narrow_bind(&mut self, ty_template: &TyTemplate, destination: Local) {
        unwrap_infallible(PullSink::is_type(self, ty_template));
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
    fn emit_terminator(&mut self, term: &Terminator) {
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
                // PopJumpIfFalse to else_block (pops condition from stack).
                // Apply jump threading to resolve through empty blocks.
                let resolved_else = self.resolve_pending_target(*else_block);
                let else_jump = self.emit(Instruction::PopJumpIfFalse(0));
                self.pending_jumps.push((else_jump, resolved_else));
                // Jump to then_block (may be elided if it's next).
                self.emit_jump_unless_fallthrough(*then_block);
            }

            Terminator::NarrowBind {
                source,
                ty_template,
                destination,
                then_block,
                else_block,
            } => {
                self.emit_operand_pull(source);
                self.emit_narrow_bind(ty_template, *destination);
                let resolved_else = self.resolve_pending_target(*else_block);
                let else_jump = self.emit(Instruction::PopJumpIfFalse(0));
                self.pending_jumps.push((else_jump, resolved_else));
                self.emit_jump_unless_fallthrough(*then_block);
            }

            Terminator::Switch {
                discriminant,
                arms,
                otherwise,
                exhaustive,
                arm_names,
            } => {
                // Build name lookup for symbolic display of discriminant values
                let name_map: std::collections::HashMap<i64, &str> =
                    arm_names.iter().map(|(v, n)| (*v, n.as_str())).collect();

                // Analyze the switch to determine the best emission strategy
                let strategy = analyze_switch(arms);

                match strategy {
                    SwitchStrategy::JumpTable { min, max } => {
                        self.emit_switch_jump_table(
                            discriminant,
                            arms,
                            *otherwise,
                            min,
                            max,
                            &name_map,
                        );
                    }
                    SwitchStrategy::PerfectHash(hash_result) => {
                        self.emit_switch_perfect_hash(
                            discriminant,
                            arms,
                            *otherwise,
                            hash_result,
                            &name_map,
                        );
                    }
                    SwitchStrategy::BinarySearch => {
                        self.emit_switch_binary_search(
                            discriminant,
                            arms,
                            *otherwise,
                            *exhaustive,
                            &name_map,
                        );
                    }
                    SwitchStrategy::IfElseChain => {
                        self.emit_switch_if_else(
                            discriminant,
                            arms,
                            *otherwise,
                            *exhaustive,
                            &name_map,
                        );
                    }
                }
            }

            Terminator::Return => {
                // Use pull model for return value - if _0 is Virtual, inline it
                unwrap_infallible(pull_semantics::walk_return_value(self));
                self.emit(Instruction::Return);
            }

            Terminator::Call {
                callee,
                args,
                ntypeargs,
                runtime_type_check,
                runtime_id,
                destination,
                target,
                unwind: _,
            } => {
                let call_span = self.current_debug_span;
                let func_name = pull_semantics::resolve_constant_function_name(
                    callee,
                    &self.analysis.classifications,
                    &self.analysis.def_use,
                );
                let global_callee = func_name
                    .as_ref()
                    .and_then(|name| self.globals.get(name).copied())
                    .map(GlobalIndex::from_raw);

                if let Some(global_callee) = global_callee {
                    unwrap_infallible(pull_semantics::walk_call_direct_args(self, args));
                    if let Some(runtime_id) = runtime_id {
                        unwrap_infallible(pull_semantics::walk_operand_pull(self, runtime_id));
                    }
                    let instruction = if runtime_id.is_some() {
                        Instruction::CallWithRuntimeId {
                            callee: global_callee,
                            ntypeargs: bex_vm_types::bytecode::encode_call_type_args(
                                *ntypeargs,
                                *runtime_type_check,
                            ),
                        }
                    } else {
                        Instruction::Call {
                            callee: global_callee,
                            ntypeargs: bex_vm_types::bytecode::encode_call_type_args(
                                *ntypeargs,
                                *runtime_type_check,
                            ),
                        }
                    };
                    // Pulling nested argument producers may install their own
                    // debug spans. Restore the terminator's enclosing call span
                    // on the actual call opcode so native diagnostics identify
                    // the offending call rather than its final nested operand.
                    self.set_debug_span(call_span, false);
                    let inst = self.emit(instruction);
                    if let Some(name) = &func_name {
                        self.set_operand(inst, OperandMeta::Callable(name.clone()));
                    }
                    self.emit_store_place(destination);
                    self.emit_jump_unless_fallthrough(*target);
                } else {
                    unwrap_infallible(pull_semantics::walk_call_indirect_operands(
                        self, callee, args,
                    ));
                    if let Some(runtime_id) = runtime_id {
                        unwrap_infallible(pull_semantics::walk_operand_pull(self, runtime_id));
                        self.set_debug_span(call_span, false);
                        self.emit(Instruction::CallIndirectWithRuntimeId);
                    } else {
                        self.set_debug_span(call_span, false);
                        self.emit(Instruction::CallIndirect);
                    }
                    self.emit_store_place(destination);
                    self.emit_jump_unless_fallthrough(*target);
                }
            }

            Terminator::VirtualCall {
                iface,
                method,
                args,
                ntypeargs,
                runtime_type_check,
                runtime_id,
                destination,
                target,
                unwind: _,
            } => {
                // Push the method type args then the value args (receiver first),
                // then the interface type, then the method name — the layout
                // `OpCode::VirtualCall` expects: it pops the method name, then the
                // interface, then the `ntypeargs` method type args, then reads the
                // receiver (first value arg) to resolve the impl at runtime.
                unwrap_infallible(pull_semantics::walk_call_direct_args(self, args));
                let iface_const = self.add_constant(ConstValue::Type(iface.to_template()));
                let inst = self.emit(Instruction::LoadType(iface_const));
                self.set_operand(inst, OperandMeta::Const(iface.to_string()));
                self.emit_constant(&Constant::String(method.clone()));
                if let Some(runtime_id) = runtime_id {
                    unwrap_infallible(pull_semantics::walk_operand_pull(self, runtime_id));
                }
                let nargs = args.len() - ntypeargs;
                let instruction = if runtime_id.is_some() {
                    Instruction::VirtualCallWithRuntimeId {
                        nargs: u16::try_from(nargs).expect("nargs fits in u16"),
                        ntypeargs: bex_vm_types::bytecode::encode_call_type_args(
                            *ntypeargs,
                            *runtime_type_check,
                        ),
                    }
                } else {
                    Instruction::VirtualCall {
                        nargs: u16::try_from(nargs).expect("nargs fits in u16"),
                        ntypeargs: bex_vm_types::bytecode::encode_call_type_args(
                            *ntypeargs,
                            *runtime_type_check,
                        ),
                    }
                };
                let inst = self.emit(instruction);
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
                runtime_id,
                destination,
                target,
                unwind: _,
            } => {
                let func_name = pull_semantics::resolve_constant_function_name(
                    callee,
                    &self.analysis.classifications,
                    &self.analysis.def_use,
                );
                let global_callee = func_name
                    .as_ref()
                    .and_then(|name| self.globals.get(name).copied())
                    .map(GlobalIndex::from_raw)
                    .unwrap_or_else(|| {
                        panic!(
                            "sys_op callee must resolve to a statically-known global function: {callee:?}"
                        )
                    });

                unwrap_infallible(pull_semantics::walk_call_direct_args(self, args));
                if let Some(runtime_id) = runtime_id {
                    unwrap_infallible(pull_semantics::walk_operand_pull(self, runtime_id));
                }
                let inst = if runtime_id.is_some() {
                    self.emit(Instruction::SysOpWithRuntimeId(global_callee))
                } else {
                    self.emit(Instruction::SysOp(global_callee))
                };
                if let Some(name) = &func_name {
                    self.set_operand(inst, OperandMeta::Callable(name.clone()));
                }
                self.emit_store_place(destination);
                self.emit_jump_unless_fallthrough(*target);
            }

            Terminator::Spawn {
                closure,
                name,
                config,
                future_ty,
                future,
                resume,
            } => {
                // Push closure, name, config, then the future's `T`/`E`. The
                // runtime `OpCode::Spawn` pops them in reverse. Config is null
                // when there is no `with` clause, so a fixed five values are
                // always pushed (BEP-034 spawn options).
                self.emit_operand_pull(closure);
                self.emit_operand_pull(name);
                let null_config = Operand::Constant(Constant::Null);
                self.emit_operand_pull(config.as_deref().unwrap_or(&null_config));
                unwrap_infallible(self.load_type(&future_ty.returns));
                unwrap_infallible(self.load_type(&future_ty.throws));
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
                unwrap_infallible(pull_semantics::walk_await_future(self, future));
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

            Terminator::Throw { value } => {
                self.emit_operand_pull(value);
                self.emit(Instruction::Throw);
            }
            Terminator::Rethrow { value } => {
                self.emit_operand_pull(value);
                self.emit(Instruction::Rethrow);
            }

            Terminator::ThrowIfPanic { value, otherwise } => {
                self.emit_operand_pull(value);
                self.emit(Instruction::ThrowIfPanic);
                self.emit_jump_unless_fallthrough(*otherwise);
            }

            Terminator::ShortCircuit {
                operand,
                is_and,
                destination,
                eval_rhs,
                join,
            } => {
                // Short-circuit lowering using JumpIfFalse (peek, no pop).
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

                if *is_and {
                    // &&: false → short-circuit edge (materialize dest), jump to join.
                    //     true → pop, evaluate rhs.
                    let sc_jump = self.emit(Instruction::JumpIfFalse(0));
                    let resolved_join = self.resolve_pending_target(*join);
                    if store_on_taken_path {
                        self.emit(Instruction::Pop(1));
                        self.emit_jump_always(*eval_rhs);
                        let taken_pc = self.bytecode.instructions.len();
                        self.patch_jump_to(sc_jump, taken_pc);
                        self.emit_store_place(destination);
                        let join_jump = self.emit(Instruction::Jump(0));
                        self.pending_jumps.push((join_jump, resolved_join));
                    } else {
                        self.pending_jumps.push((sc_jump, resolved_join));
                        self.emit(Instruction::Pop(1));
                        self.emit_jump_unless_fallthrough(*eval_rhs);
                    }
                } else {
                    // ||: false → pop, evaluate rhs.
                    //     true → short-circuit edge (materialize dest), jump to join.
                    let false_jump = self.emit(Instruction::JumpIfFalse(0));
                    let resolved_join = self.resolve_pending_target(*join);
                    if store_on_taken_path {
                        self.emit_store_place(destination);
                    }
                    let true_jump = self.emit(Instruction::Jump(0));
                    self.pending_jumps.push((true_jump, resolved_join));
                    // False landing: patch JumpIfFalse to here, pop, fall to eval_rhs.
                    let false_pc = self.bytecode.instructions.len();
                    self.patch_jump_to(false_jump, false_pc);
                    self.emit(Instruction::Pop(1));
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

    /// Build the bytecode exception table from MIR catch regions.
    ///
    /// Each `CatchRegion` contributes the exact PC ranges of its protected
    /// `body_blocks` (coalesced where the layout made them contiguous), NOT a
    /// single `[body_entry_pc, handler_pc)` span. A span-based table is only
    /// correct if the layout places every protected block before the handler
    /// and every unprotected block outside the span — reverse-postorder
    /// guarantees neither (a direct-throw block is a CFG leaf that sinks past
    /// the handler; a panic-capable call-free block has no unwind edge to
    /// anchor it), and both escaped their `catch` before this was made exact.
    ///
    /// Nested regions overlap: the protected PC set of an inner region is a
    /// subset of every enclosing region's (inner windows nest inside outer
    /// windows at lowering). The VM picks the innermost covering entry —
    /// largest `start_pc`, then smallest `end_pc`, then latest table order —
    /// which subset-nesting makes unambiguous: the inner region's coalesced
    /// range around any PC is contained in the outer's, and for byte-identical
    /// ranges the stable sort below preserves `catch_regions` creation order
    /// (always outer before inner), so the last matching entry is the inner
    /// handler.
    fn build_exception_table(&mut self, mir: &MirFunctionBody) {
        use bex_vm_types::bytecode::{ExceptionTableEntry, HandlerContextEntry};

        for region in &mir.catch_regions {
            let handler = self.analysis.resolve_jump_target(region.handler);

            let &handler_pc = self.block_addresses.get(&handler).unwrap_or_else(|| {
                unreachable!(
                    "exception table: handler block {handler:?} has no PC address — \
                     catch region was emitted but its handler block was dropped"
                )
            });
            // If the error local was optimized away (e.g. an inline
            // `throw X catch ...` that the MIR lowers as a direct jump),
            // the catch region doesn't need a VM-level exception table entry.
            let Some(&error_slot) = self.local_slots.get(&region.error_local) else {
                log::debug!(
                    "exception table: error local {:?} has no slot (optimized away)",
                    region.error_local,
                );
                continue;
            };

            let stack_trace_slot = region
                .stack_trace_local
                .and_then(|local| self.local_slots.get(&local).copied())
                .unwrap_or(ExceptionTableEntry::NO_STACK_TRACE);

            // BEP-042 cause chain: a throw inside the handler body is "during
            // handling of" this catch's error. The handler body is the union of
            // the arm blocks (or defer-pad body blocks) captured at lowering;
            // the layout can fragment them across non-contiguous PCs. Emit one
            // `HandlerContextEntry` per block so the coverage is exact — a
            // single `[handler_pc, max_end)` span would over-cover the gaps
            // between fragments and mis-chain a throw laid out there. An empty
            // or fully-dropped body contributes no entries and never chains.
            for &block in &region.handler_body {
                let (Some(&block_start), Some(&block_end)) = (
                    self.block_addresses.get(&block),
                    self.block_end_addresses.get(&block),
                ) else {
                    continue; // block dropped by layout / DCE
                };
                if block_start >= block_end {
                    continue; // empty block — nothing to cover
                }
                self.bytecode
                    .handler_context_table
                    .push(HandlerContextEntry {
                        start_pc: block_start,
                        end_pc: block_end,
                        handler_pc,
                        stack_trace_slot,
                    });
            }

            // Exact protected coverage: one PC range per protected block,
            // merged where the layout put member blocks back-to-back. Blocks
            // dropped by layout/DCE have no addresses and nothing to protect;
            // gaps between member fragments (e.g. an interleaved handler or
            // post-join block) stay uncovered by construction.
            let mut ranges: Vec<(usize, usize)> = region
                .body_blocks
                .iter()
                .filter_map(|&block| {
                    let &block_start = self.block_addresses.get(&block)?;
                    let &block_end = self.block_end_addresses.get(&block)?;
                    (block_start < block_end).then_some((block_start, block_end))
                })
                .collect();
            ranges.sort_unstable();
            let mut coalesced: Vec<(usize, usize)> = Vec::new();
            for (start, end) in ranges {
                match coalesced.last_mut() {
                    Some(last) if start <= last.1 => last.1 = last.1.max(end),
                    _ => coalesced.push((start, end)),
                }
            }
            for (start_pc, end_pc) in coalesced {
                self.bytecode.exception_table.push(ExceptionTableEntry {
                    start_pc,
                    end_pc,
                    handler_pc,
                    error_slot,
                    stack_trace_slot,
                });
            }
        }

        // Stable sort by start_pc: the VM selects the innermost covering entry
        // by (largest start_pc, smallest end_pc, latest table order); stability
        // keeps outer-before-inner creation order for byte-identical ranges so
        // "latest" resolves to the inner handler (see the function doc).
        self.bytecode.exception_table.sort_by_key(|e| e.start_pc);
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
        discriminant: &Operand,
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
        discriminant: &Operand,
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

    /// Emit switch using binary search (O(log n) comparisons).
    ///
    /// Creates a balanced binary search tree of comparisons.
    ///
    /// Note: The exhaustive optimization is not applied to binary search because
    /// the savings are minimal (O(1) instruction in O(log n) total) and the
    /// implementation would be complex (need to track rightmost leaf of tree).
    fn emit_switch_binary_search(
        &mut self,
        discriminant: &Operand,
        arms: &[(i64, BlockId)],
        otherwise: BlockId,
        _exhaustive: bool,
        name_map: &std::collections::HashMap<i64, &str>,
    ) {
        // Push discriminant onto stack (will be popped by comparisons)
        self.emit_operand_pull(discriminant);

        // Sort arms by value for binary search
        let mut sorted_arms: Vec<_> = arms.to_vec();
        sorted_arms.sort_by_key(|(v, _)| *v);

        // Emit binary search tree
        self.emit_binary_search_node(&sorted_arms, otherwise, name_map);

        // Pop the discriminant if we fall through (shouldn't happen with well-formed switches)
        self.emit(Instruction::Pop(1));
        self.emit_jump_unless_fallthrough(otherwise);
    }

    /// Recursively emit a binary search node.
    ///
    /// The discriminant is already on the stack. We emit comparisons to split
    /// the search space in half at each level.
    #[allow(clippy::only_used_in_recursion)]
    fn emit_binary_search_node(
        &mut self,
        arms: &[(i64, BlockId)],
        otherwise: BlockId,
        name_map: &std::collections::HashMap<i64, &str>,
    ) {
        match arms.len() {
            0 => {
                // No arms left - just fall through to otherwise
                // (already handled by caller)
            }
            1 | 2 => {
                // One or two arms - emit direct comparisons sequentially
                for (value, target) in arms {
                    let label = Self::switch_label(*value, name_map);
                    self.emit_compare_and_branch(*value, *target, label);
                }
            }
            _ => {
                // Multiple arms - split in half and recurse
                let mid = arms.len() / 2;
                let (value, target) = &arms[mid];
                let left = &arms[..mid];
                let right = &arms[mid + 1..];

                // Compare with pivot — if equal, pop discriminant and jump
                let label = Self::switch_label(*value, name_map);
                self.emit_compare_and_branch(*value, *target, label.clone());

                // Compare < pivot for left subtree
                self.emit(Instruction::Copy(0));
                let idx = self.add_constant(ConstValue::Int(*value));
                let inst = self.emit(Instruction::LoadConst(idx));
                self.set_operand(inst, OperandMeta::Const(label));
                self.emit(Instruction::CmpIntOp(CmpOp::Lt));
                let lt_jump = self.emit(Instruction::PopJumpIfFalse(0));

                // Left subtree (values < pivot)
                self.emit_binary_search_node(left, otherwise, name_map);

                let after_left = self.current_pc();
                self.patch_jump_to(lt_jump, after_left);

                // Right subtree (values > pivot)
                self.emit_binary_search_node(right, otherwise, name_map);
            }
        }
    }

    /// Emit switch using perfect hash + dense jump table (O(1) dispatch).
    ///
    /// The `MatchHash` instruction remaps the sparse type tag to a dense
    /// `[0, K-1]` index via a compile-time minimal perfect hash. A subsequent
    /// `JumpTable` dispatches on the dense index. `MatchHash` pushes `-1` for
    /// unknown tags, which falls to the jump table's default arm.
    ///
    /// This replaces O(log K) `BinarySearch` (which emits ~4K instructions for
    /// K=8) with just 3 instructions: `type_tag` + `match_hash` + `jump_table`.
    ///
    /// The perfect hash uses the multiply-shift family:
    ///   `h(tag) = ((tag as u64).wrapping_mul(M) >> S) & mask`
    ///
    /// References:
    /// - Neumann & Göbbert, "Improving Switch Statement Performance with
    ///   Hashing Optimized at Compile Time"
    /// - Dietz 1992, "Coding Multiway Branches Using Customized Hash Functions"
    /// - Proposed for LLVM (#96971), Roslyn (#66604), Go (#34381)
    #[allow(clippy::cast_possible_wrap)]
    fn emit_switch_perfect_hash(
        &mut self,
        discriminant: &Operand,
        arms: &[(i64, BlockId)],
        otherwise: BlockId,
        hash_result: PerfectHashResult,
        name_map: &std::collections::HashMap<i64, &str>,
    ) {
        // 1. Push discriminant (type tag) onto stack — consumed by DenseTag.
        self.emit_operand_pull(discriminant);

        // 2. Store the MatchHashTable in bytecode and emit DenseTag instruction.
        let key_names: Vec<String> = arms
            .iter()
            .map(|(v, _)| {
                name_map
                    .get(v)
                    .map(ToString::to_string)
                    .unwrap_or_else(|| v.to_string())
            })
            .collect();
        let table = MatchHashTable {
            multiply: hash_result.multiply,
            shift: hash_result.shift,
            mask: hash_result.mask,
            entries: hash_result.entries,
            key_names,
        };
        let hash_table_idx = self.bytecode.match_hash_tables.len();
        self.bytecode.match_hash_tables.push(table);
        self.emit(Instruction::DenseTag(hash_table_idx));

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
        for (dense_idx, (orig_value, _)) in arms.iter().enumerate() {
            if let Some(&name) = name_map.get(orig_value) {
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

    /// Emit: copy TOS, compare with `value` for equality; if equal, pop
    /// discriminant and jump to `target`. On mismatch, fall through.
    ///
    /// Used by binary search where the discriminant stays on the stack across
    /// the tree traversal. The if-else chain uses a different strategy
    /// (re-loading the discriminant per arm) to avoid copy/pop overhead.
    fn emit_compare_and_branch(&mut self, value: i64, target: BlockId, label: String) {
        self.emit(Instruction::Copy(0));
        let idx = self.add_constant(ConstValue::Int(value));
        let inst = self.emit(Instruction::LoadConst(idx));
        self.set_operand(inst, OperandMeta::Const(label));
        self.emit(Instruction::CmpIntOp(CmpOp::Eq));
        let jump_idx = self.emit(Instruction::PopJumpIfFalse(0));
        self.emit(Instruction::Pop(1));
        self.emit_jump_always(target);
        let skip_to = self.current_pc();
        self.patch_jump_to(jump_idx, skip_to);
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
        }
    }

    /// Build local variable name mapping from MIR and slot assignments.
    ///
    /// Returns a flat `Vec<String>` mapping slot indices to variable names.
    fn build_local_names(
        mir: &MirFunctionBody,
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
        mir: &MirFunctionBody,
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

impl PullSink for StackifyCodegen<'_, '_> {
    type Error = Infallible;

    fn pull_constant(&mut self, constant: &Constant) -> Result<(), Self::Error> {
        self.emit_constant(constant);
        Ok(())
    }

    fn pull_local(&mut self, local: Local) -> Result<LocalPullAction, Self::Error> {
        let classification = self.analysis.classifications[&local];

        let action = match classification {
            LocalClassification::Virtual => {
                // Attribute inlined virtual loads to their defining statement.
                self.set_debug_span(self.def_span_for_local(local), false);
                // Inline the definition rvalue at use site.
                let rvalue = self.analysis.def_use[&local]
                    .def
                    .as_ref()
                    .map(|def| def.rvalue.clone())
                    .unwrap_or_else(|| panic!("virtual local {local} without definition"));
                // MakeClosure must be handled specially: its captures need to load
                // cell pointers (LoadVar) not cell values (LoadDeref). We intercept
                // here so that `emit_rvalue_pull` (which sets loading_for_closure_capture)
                // is called rather than the generic `walk_rvalue_pull` inlining path.
                // MakeBoundMethod / MakeVirtualBoundMethod / VirtualFieldAccess must
                // also be handled specially: none is handled by `walk_rvalue_pull`
                // (which panics on them), so route through `emit_rvalue_pull`.
                // BinaryOp must be routed through `emit_rvalue_pull` so that the
                // type-aware specialization in `try_specialize_binary_op` can fire
                // (e.g. emitting `CmpBigintOp` instead of the generic `CmpOp`).
                // Class aggregates may use emitter-only spread helpers, so they
                // also need to flow through `emit_rvalue_pull` when inlined.
                if matches!(
                    rvalue,
                    Rvalue::MakeClosure { .. }
                        | Rvalue::MakeBoundMethod { .. }
                        | Rvalue::MakeVirtualBoundMethod { .. }
                        | Rvalue::VirtualFieldAccess { .. }
                        | Rvalue::BinaryOp { .. }
                        | Rvalue::Aggregate {
                            kind: baml_compiler2_mir::AggregateKind::Class { .. },
                            ..
                        }
                ) {
                    self.emit_rvalue_pull(&rvalue);
                    return Ok(LocalPullAction::Done);
                }
                LocalPullAction::Inline(Box::new(rvalue))
            }
            LocalClassification::PhiLike
            | LocalClassification::ReturnPhi
            | LocalClassification::CallResultImmediate
            | LocalClassification::AggregateOperand => LocalPullAction::Done,
            LocalClassification::CopyOf => {
                // Copy propagation: load from source slot directly.
                let source = self.analysis.resolve_copy_source(local);
                let slot = self.local_slots[&source];
                if self.captured_locals.contains(&source) && !self.loading_for_closure_capture {
                    self.emit(Instruction::LoadDeref(slot));
                } else {
                    self.emit_load_var(slot);
                }
                LocalPullAction::Done
            }
            LocalClassification::Parameter
            | LocalClassification::Real
            | LocalClassification::Dead => {
                let slot = self.local_slots[&local];
                if self.captured_locals.contains(&local) && !self.loading_for_closure_capture {
                    // Captured local: load the value through the cell.
                    self.emit(Instruction::LoadDeref(slot));
                } else {
                    // Normal local or loading cell pointer for MakeClosure.
                    self.emit_load_var(slot);
                }
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

    fn binary_op(&mut self, op: BinOp) -> Result<(), Self::Error> {
        self.emit(Self::binop_instruction(op));
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
        let deep_copy_idx = self
            .globals
            .get("baml.deep_copy")
            .copied()
            .unwrap_or_else(|| panic!("undefined function: baml.deep_copy"));
        let inst = self.emit(Instruction::Call {
            callee: GlobalIndex::from_raw(deep_copy_idx),
            ntypeargs: 0,
        });
        self.set_operand(inst, OperandMeta::Callable("baml.deep_copy".to_string()));
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
        class_name: &str,
        ntypeargs: u16,
    ) -> Result<(), Self::Error> {
        if let Some(&class_obj_idx) = self.class_object_indices.get(class_name) {
            let inst = self.emit(Instruction::AllocInstance {
                class_obj: ObjectIndex::from_raw(class_obj_idx),
                ntypeargs,
            });
            self.set_operand(inst, OperandMeta::Object(class_name.to_string()));
        } else {
            // Class not found — this can happen when the parser produces an
            // anonymous or misidentified object literal (e.g., `null { }` from
            // an ambiguous if-condition). Emit a null constant as a fallback so
            // compilation doesn't panic; the runtime behavior is best-effort.
            let null_idx = self.add_constant(bex_vm_types::ConstValue::Null);
            let inst = self.emit(bex_vm_types::Instruction::LoadConst(null_idx));
            self.set_operand(
                inst,
                OperandMeta::Const(format!("null /* unknown class: {class_name} */")),
            );
        }
        Ok(())
    }

    fn init_class_instance(
        &mut self,
        class_name: &str,
        ntypeargs: u16,
        field_count: usize,
    ) -> Result<(), Self::Error> {
        self.emit_init_instance(class_name, ntypeargs, field_count);
        Ok(())
    }

    fn init_field(&mut self, field_idx: usize, name: &str) -> Result<(), Self::Error> {
        let idx = self.emit(Instruction::InitField(field_idx));
        self.set_operand(idx, OperandMeta::Field(name.to_string()));
        Ok(())
    }

    fn alloc_enum_variant(&mut self, enum_name: &str, variant: &str) -> Result<(), Self::Error> {
        let enum_obj_idx = self
            .enum_object_indices
            .get(enum_name)
            .copied()
            .unwrap_or_else(|| panic!("undefined enum: {enum_name}"));

        let variant_idx = self
            .enum_variants
            .get(enum_name)
            .and_then(|variants| variants.get(variant))
            .copied()
            .unwrap_or_else(|| panic!("undefined variant: {enum_name}.{variant}"));

        #[allow(clippy::cast_possible_wrap)]
        let idx = self.add_constant(ConstValue::Int(variant_idx as i64));
        let lc_inst = self.emit(Instruction::LoadConst(idx));
        self.set_operand(
            lc_inst,
            OperandMeta::Const(format!("{enum_name}.{variant}")),
        );
        let inst = self.emit(Instruction::AllocVariant(ObjectIndex::from_raw(
            enum_obj_idx,
        )));
        self.set_operand(inst, OperandMeta::Object(enum_name.to_string()));
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

    fn is_type(&mut self, ty_template: &TyTemplate) -> Result<(), Self::Error> {
        let emit_false = |this: &mut Self| {
            this.emit(Instruction::Pop(1));
            let idx = this.add_constant(ConstValue::Bool(false));
            let inst = this.emit(Instruction::LoadConst(idx));
            this.set_operand(inst, OperandMeta::Const("false".to_string()));
        };
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
            let c = this.add_constant(ConstValue::Type(template.clone()));
            let inst = this.emit(Instruction::IsType(c));
            this.set_operand(inst, OperandMeta::Const(template.to_string()));
        };
        match ty_template {
            // ── Class check ──────────────────────────────────────────────────
            // Every class (monomorphic `Foo`, concrete `Foo<int>`, or generic
            // `Foo<T>`) is a `Class` template. Non-empty args → `ClassWithTypeArgs`
            // so the VM compares each arg invariantly; empty args →
            // class-pointer identity.
            TyTemplate::Class(tn, type_args_templates, _) => {
                // A reflected `type` value is physically `Object::Type` but its
                // reconstructed concrete type is one of the nine sealed kind
                // classes. Kind tests must therefore use the structural value
                // matcher; class-object pointer identity only applies to normal
                // user instances.
                if baml_type::type_kind::is_type_kind_class(tn) {
                    emit_structural(self, ty_template);
                    return Ok(());
                }
                let class_name_str = tn.display_name();
                let Some(class_obj_idx) = self.class_object_index_for_type_name(tn) else {
                    emit_false(self);
                    return Ok(());
                };
                if type_args_templates.is_empty() {
                    let c =
                        self.add_constant(ConstValue::Object(ObjectIndex::from_raw(class_obj_idx)));
                    let inst = self.emit(Instruction::IsType(c));
                    self.set_operand(inst, OperandMeta::Const(class_name_str.to_string()));
                } else {
                    let c = self.add_constant(ConstValue::ClassWithTypeArgs {
                        class_obj: ObjectIndex::from_raw(class_obj_idx),
                        type_args_templates: type_args_templates.clone(),
                    });
                    let inst = self.emit(Instruction::IsType(c));
                    self.set_operand(inst, OperandMeta::Const(format!("{class_name_str}<...>")));
                }
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
            TyTemplate::BuiltinUnknown { .. } => emit_true(self),

            // ── Singleton (literal) ──────────────────────────────────────────
            // A literal type is a set of one, so membership is decided against
            // the value itself, not against a type tag: every tag a literal
            // could name is its *base* type's, which answers `true` for every
            // other inhabitant of that base (`x is One` matching every int when
            // `type One = 1`). `ConstValue::Literal` is the exact test — the
            // specialization of the `ConstValue::Type(Literal)` structural form
            // the algebra would otherwise decide, minus the reconstruction.
            TyTemplate::Literal(literal, _, _) => {
                let c = self.add_constant(ConstValue::Literal(literal.clone()));
                let inst = self.emit(Instruction::IsType(c));
                self.set_operand(inst, OperandMeta::Const(literal.to_string()));
            }

            // Fully realized leaves keep their exact identity/tag fast path,
            // then use structural matching when no exact fast path exists.
            // This list is exhaustive on purpose: a new template variant must
            // choose its type-test strategy here.
            other @ (TyTemplate::Int { .. }
            | TyTemplate::Bigint { .. }
            | TyTemplate::Float { .. }
            | TyTemplate::String { .. }
            | TyTemplate::Bool { .. }
            | TyTemplate::Null { .. }
            | TyTemplate::Uint8Array { .. }
            | TyTemplate::Enum(..)
            | TyTemplate::EnumVariant(..)
            | TyTemplate::RustType { .. }
            | TyTemplate::Type { .. }
            | TyTemplate::Resource { .. }
            | TyTemplate::PromptAst { .. }
            | TyTemplate::Void { .. }
            | TyTemplate::TypeAlias(..)
            | TyTemplate::Never { .. }) => {
                // A fully-realized leaf (primitive, enum, alias, literal, ...):
                // class-pointer identity for a `TypeAlias`, otherwise its type
                // tag when one exactly represents the test. Tagless leaves use
                // the canonical structural matcher instead of silently
                // compiling to false.
                let realized = <&RealizedTy>::try_from(other)
                    .expect("exhaustive realized-leaf template classification");
                if let RealizedTy::TypeAlias(tn, _) = realized {
                    if let Some(class_obj_idx) = self.class_object_index_for_type_name(tn) {
                        let c = self
                            .add_constant(ConstValue::Object(ObjectIndex::from_raw(class_obj_idx)));
                        let inst = self.emit(Instruction::IsType(c));
                        self.set_operand(inst, OperandMeta::Const(tn.display_name().to_string()));
                    } else {
                        emit_false(self);
                    }
                } else if let RealizedTy::Enum(tn, _) = realized {
                    // Enum-pointer identity: `is Color` tests the value's enum
                    // object, so it discriminates `Color` from `Status` - the
                    // shared `ENUM` type tag cannot. Falls back to constant-false
                    // if the enum object is absent (e.g. an unreferenced enum).
                    if let Some(enum_obj_idx) = self.enum_object_index_for_type_name(tn) {
                        let c = self
                            .add_constant(ConstValue::Object(ObjectIndex::from_raw(enum_obj_idx)));
                        let inst = self.emit(Instruction::IsType(c));
                        self.set_operand(inst, OperandMeta::Const(tn.display_name().to_string()));
                    } else {
                        emit_false(self);
                    }
                } else if let Some(tag) = realized_type_tag(realized) {
                    let c = self.add_constant(ConstValue::Int(tag));
                    let inst = self.emit(Instruction::IsType(c));
                    self.set_operand(inst, OperandMeta::Const(realized.to_string()));
                } else {
                    emit_structural(self, other);
                }
            }
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

    fn runtime_is_type(&mut self) -> Result<(), Self::Error> {
        self.emit(Instruction::RuntimeIsType);
        Ok(())
    }

    fn load_type(&mut self, template: &TyTemplate) -> Result<(), Self::Error> {
        let const_idx = self.add_constant(ConstValue::Type(template.clone()));
        let inst = self.emit(Instruction::LoadType(const_idx));
        self.set_operand(inst, OperandMeta::Const(template.to_string()));
        Ok(())
    }

    fn load_current_package(&mut self, package: &str) -> Result<(), Self::Error> {
        let object = self.mint_object(Object::String(package.into()));
        let constant = self.add_constant(ConstValue::Object(ObjectIndex::from_raw(object)));
        let inst = self.emit(Instruction::LoadCurrentPackage(constant));
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
        item: &baml_compiler2_mir::ItemRef,
        ntypeargs: usize,
    ) -> Result<(), Self::Error> {
        let func_name = item.to_string();
        let global_idx = *self
            .globals
            .get(&func_name)
            .unwrap_or_else(|| panic!("MakeGenericFunction: global not found for {func_name}"));
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

    fn load_capture(&mut self, idx: usize) -> Result<(), Self::Error> {
        if self.loading_for_closure_capture {
            // When building a MakeClosure capture list, we want to forward the
            // raw cell pointer from this closure's capture slot to the inner
            // closure — not read through the cell to get the inner value.
            self.emit(Instruction::CaptureRef(idx));
        } else {
            self.emit(Instruction::LoadCapture(idx));
        }
        Ok(())
    }

    fn resolve_field_name(&self, base: &Place, field_idx: usize) -> String {
        let class_name = match self.resolve_place_type(base) {
            Some(RuntimeTy::Class(tn, _, _)) => tn.display_name().to_string(),
            _ => return format!("{field_idx}"),
        };
        self.lookup_class_field_name(&class_name, field_idx)
            .unwrap_or_else(|| format!("{field_idx}"))
    }

    fn class_field_name(&self, class_name: &str, field_idx: usize) -> String {
        self.lookup_class_field_name(class_name, field_idx)
            .unwrap_or_else(|| format!("{field_idx}"))
    }
}

impl StackEffectSink for StackifyCodegen<'_, '_> {
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

    fn store_capture_value(&mut self, idx: usize) -> Result<(), Self::Error> {
        self.emit(Instruction::StoreCapture(idx));
        Ok(())
    }
}

/// The coarse `IsType` type tag for a realized leaf type, or `None` for a type
/// with no representable tag (classes take the pointer-identity path instead).
fn realized_type_tag(ty: &RealizedTy) -> Option<i64> {
    match ty {
        RealizedTy::Int { .. } => Some(baml_type::typetag::INT),
        RealizedTy::Bigint { .. } => Some(baml_type::typetag::BIGINT),
        RealizedTy::String { .. } => Some(baml_type::typetag::STRING),
        RealizedTy::Bool { .. } => Some(baml_type::typetag::BOOL),
        RealizedTy::Null { .. } => Some(baml_type::typetag::NULL),
        RealizedTy::Float { .. } => Some(baml_type::typetag::FLOAT),
        RealizedTy::Enum(..) => Some(baml_type::typetag::ENUM),
        RealizedTy::List(..) => Some(baml_type::typetag::LIST),
        RealizedTy::Map { .. } => Some(baml_type::typetag::MAP),
        RealizedTy::Function { .. } => Some(baml_type::typetag::FUNCTION),
        RealizedTy::Type { .. } => Some(baml_type::typetag::TYPE),
        RealizedTy::Uint8Array { .. } => Some(baml_type::typetag::UINT8ARRAY),
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
        | RealizedTy::RustType { .. }
        | RealizedTy::Resource { .. }
        | RealizedTy::PromptAst { .. }
        | RealizedTy::Void { .. }
        | RealizedTy::TypeAlias(..)
        | RealizedTy::BuiltinUnknown { .. }
        | RealizedTy::Never { .. }
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
pub(crate) fn compile_mir_function<'mir>(
    body: &'mir MirFunctionBody,
    arity: usize,
    mir_span: Option<baml_base::Span>,
    line_starts: &'mir [u32],
    ctx: MirCodegenContext<'mir, '_>,
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
        BasicBlock, BlockId, Constant, Local, LocalDecl, MirFunctionBody, Operand, Place, Rvalue,
        Statement, StatementKind, Terminator,
    };
    use baml_type::RuntimeTy;
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
            catch_regions: Vec::new(),
            viz_nodes: Vec::new(),
        };

        let globals = HashMap::new();
        let classes = HashMap::new();
        let class_object_indices = HashMap::new();
        let enum_object_indices = HashMap::new();
        let enum_variants = HashMap::new();
        let class_fields = HashMap::new();
        let mut objects = ObjectPool::default();
        let lambda_object_indices = Vec::new();
        let lambda_names = Vec::new();
        let capture_types = Vec::new();
        let spawn_capture_indices = HashSet::new();
        let line_starts = [0];

        let function = compile_mir_function(
            &body,
            1,
            None,
            &line_starts,
            MirCodegenContext {
                globals: &globals,
                classes: &classes,
                class_object_indices: &class_object_indices,
                enum_object_indices: &enum_object_indices,
                enum_variants: &enum_variants,
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
        use super::switch_discriminant_pulls;
        let arms = |values: &[i64]| -> Vec<(i64, BlockId)> {
            values.iter().map(|&v| (v, BlockId(0))).collect()
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
        // Sparse 4+ arms: perfect hash (or binary search), single pull either way.
        assert_eq!(
            switch_discriminant_pulls(&arms(&[10, 2000, 300_000, 40_000_000]), false),
            1
        );
    }
}
