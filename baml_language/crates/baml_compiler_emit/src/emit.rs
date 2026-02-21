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
use baml_compiler_mir::{
    BasicBlock, BinOp, BlockId, Constant, IndexKind, Local, MirFunction, Operand, Place, Rvalue,
    StatementKind, Terminator, UnaryOp,
};
use baml_type::Ty;
use bex_vm_types::{
    BinOp as VmBinOp, Bytecode, CmpOp, ConstValue, Function, FunctionKind, GlobalIndex,
    Instruction, Object, ObjectIndex, ObjectPool, UnaryOp as VmUnaryOp,
    bytecode::{
        BlockNotification, BlockNotificationType, DebugLocalScope, InstructionMeta, JumpTableData,
        LineTableEntry, OperandMeta,
    },
};

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
    // Use binary search for sparse but large switch
    else if arms.len() >= BINARY_SEARCH_MIN_ARMS {
        SwitchStrategy::BinarySearch
    }
    // Default to if-else chain for small switches
    else {
        SwitchStrategy::IfElseChain
    }
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

/// MIR to bytecode compiler with stackification.
struct StackifyCodegen<'ctx, 'obj> {
    /// MIR being compiled.
    mir: &'ctx MirFunction,
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
    /// Shared object pool.
    objects: &'obj mut ObjectPool,

    /// Analysis results (classifications, def-use, etc.).
    analysis: AnalysisResult,

    /// Maps MIR Local -> stack slot index (only for Real locals).
    local_slots: HashMap<Local, usize>,

    /// Number of extra local slots required for this function frame.
    real_local_count: usize,

    /// Maps `BlockId` -> bytecode instruction index (for jump patching).
    block_addresses: HashMap<BlockId, usize>,

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

    /// Watched locals that have already had Watch instruction emitted.
    /// We only emit Watch once per watched local (at initialization).
    watched_locals_initialized: HashSet<Local>,

    /// Block notifications to be attached to the compiled function.
    block_notifications: Vec<BlockNotification>,

    /// MIR local types for field name resolution (debug info).
    local_types: HashMap<Local, Ty>,

    /// Slot index → variable name mapping for debug metadata.
    slot_names: Vec<String>,
}

impl<'ctx, 'obj> StackifyCodegen<'ctx, 'obj> {
    /// Create a new stackification codegen instance.
    #[allow(clippy::needless_pass_by_value)] // ctx is destructured into self fields
    fn new(
        mir: &'ctx MirFunction,
        line_starts: &'ctx [u32],
        ctx: MirCodegenContext<'ctx, 'obj>,
        analysis: AnalysisResult,
    ) -> Self {
        Self {
            mir,
            line_starts,
            globals: ctx.globals,
            classes: ctx.classes,
            class_object_indices: ctx.class_object_indices,
            enum_object_indices: ctx.enum_object_indices,
            enum_variants: ctx.enum_variants,
            objects: ctx.objects,
            analysis,
            local_slots: HashMap::new(),
            real_local_count: 0,
            block_addresses: HashMap::new(),
            pending_jumps: Vec::new(),
            pending_jump_tables: Vec::new(),
            dead_unreachable_blocks: HashSet::new(),
            trap_pc: None,
            bytecode: Bytecode::new(),
            current_debug_span: None,
            pending_sequence_point: false,
            next_line_discriminator: HashMap::new(),
            next_block: None,
            watched_locals_initialized: HashSet::new(),
            block_notifications: Vec::new(),
            local_types: HashMap::new(),
            slot_names: Vec::new(),
        }
    }

    /// Look up a field name from the `ObjectPool` given a class name and field index.
    fn lookup_class_field_name(&self, class_name: &str, field_idx: usize) -> Option<String> {
        let &obj_idx = self.class_object_indices.get(class_name)?;
        match self.objects.get(obj_idx)? {
            Object::Class(class) => class.fields.get(field_idx).map(|f| f.name.clone()),
            _ => None,
        }
    }

    /// Resolve the type of a MIR Place by walking from the root local through projections.
    fn resolve_place_type(&self, place: &Place) -> Option<Ty> {
        match place {
            Place::Local(local) => self.local_types.get(local).cloned(),
            Place::Field { base, field } => {
                let base_ty = self.resolve_place_type(base)?;
                match &base_ty {
                    Ty::Class(type_name) => {
                        let &obj_idx = self
                            .class_object_indices
                            .get(type_name.display_name.as_str())?;
                        match self.objects.get(obj_idx)? {
                            Object::Class(class) => {
                                class.fields.get(*field).map(|f| f.field_type.clone())
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }
            Place::Index { base, .. } => {
                let base_ty = self.resolve_place_type(base)?;
                match base_ty {
                    Ty::List(inner) => Some(*inner),
                    Ty::Map { value, .. } => Some(*value),
                    _ => None,
                }
            }
        }
    }

    fn local_slot_or_panic(&self, local: Local, context: &str) -> usize {
        *self.local_slots.get(&local).unwrap_or_else(|| {
            panic!("local {local} has no allocated slot while emitting {context}")
        })
    }

    fn span_for_statement_ref(&self, block: BlockId, statement_ref: StatementRef) -> Option<Span> {
        let block = self.mir.block(block);
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
        let mir = self.mir;
        // 1. Allocate stack slots only for real locals
        self.allocate_real_locals(mir);

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

            self.block_addresses.insert(block_id, self.current_pc());
            let block = mir.block(block_id);
            self.emit_block(block);
        }

        // If any pending edges target dead-unreachable MIR blocks, patch them
        // through a shared trap target instead of assigning fake block addresses.
        self.ensure_trap_pc_if_needed();

        // 3. Patch all jump targets and jump tables
        self.patch_jumps();
        self.patch_jump_tables();

        // 4. Convert MIR VizNodes to VM VizNodeMeta
        let viz_nodes = mir
            .viz_nodes
            .iter()
            .map(|node| bex_vm_types::VizNodeMeta {
                node_id: node.node_id,
                log_filter_key: node.log_filter_key.clone(),
                parent_log_filter_key: node.parent_log_filter_key.clone(),
                node_type: Self::convert_viz_node_type(node.node_type),
                label: node.label.clone(),
                header_level: node.header_level,
            })
            .collect();
        let debug_locals = Self::build_debug_locals(mir, &self.local_slots);

        // 5. Build the Function
        Function {
            name: mir.name.to_string(),
            arity: mir.arity,
            real_local_count: self.real_local_count,
            bytecode: self.bytecode,
            kind: FunctionKind::Bytecode,
            local_names: self.slot_names,
            debug_locals,
            span: mir.span.unwrap_or_else(Span::fake),
            block_notifications: self.block_notifications,
            viz_nodes,
            return_type: baml_type::Ty::Null,
            param_names: Vec::new(),
            param_types: Vec::new(),
            body_meta: None,
            trace: false,
        }
    }

    /// Convert MIR `VizNodeType` to VM `VizNodeType`.
    fn convert_viz_node_type(
        mir_type: baml_compiler_mir::VizNodeType,
    ) -> bex_vm_types::VizNodeType {
        match mir_type {
            baml_compiler_mir::VizNodeType::FunctionRoot => bex_vm_types::VizNodeType::FunctionRoot,
            baml_compiler_mir::VizNodeType::HeaderContextEnter => {
                bex_vm_types::VizNodeType::HeaderContextEnter
            }
            baml_compiler_mir::VizNodeType::BranchGroup => bex_vm_types::VizNodeType::BranchGroup,
            baml_compiler_mir::VizNodeType::BranchArm => bex_vm_types::VizNodeType::BranchArm,
            baml_compiler_mir::VizNodeType::Loop => bex_vm_types::VizNodeType::Loop,
            baml_compiler_mir::VizNodeType::OtherScope => bex_vm_types::VizNodeType::OtherScope,
        }
    }

    /// Allocate stack slots only for Real locals.
    ///
    /// Virtual locals don't get slots - they're inlined at use sites.
    fn allocate_real_locals(&mut self, mir: &MirFunction) {
        self.local_slots.clear();
        self.real_local_count = 0;
        let arity = mir.arity;

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
                | LocalClassification::CopyOf
                | LocalClassification::Dead => {
                    // Virtual, phi-like, return-phi, call-result-immediate, copy-of, and dead locals don't get slots!
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
                    Place::Local(local) => {
                        // Local assignment: emit rvalue then store
                        self.emit_rvalue_pull(value);
                        self.emit_store_place(destination);
                        // Emit Watch only once for watched locals (at initialization)
                        let local_decl = self.mir.local(*local);
                        if local_decl.is_watched && !self.watched_locals_initialized.contains(local)
                        {
                            self.watched_locals_initialized.insert(*local);
                            unwrap_infallible(
                                self.push_watch_channel(*local, local_decl.name.as_deref()),
                            );
                            let null_const_idx = self.add_constant(ConstValue::Null);
                            let inst = self.emit(Instruction::LoadConst(null_const_idx));
                            self.set_operand(inst, OperandMeta::Const("null".to_string()));
                            unwrap_infallible(self.watch_local(*local));
                        }
                    }
                    Place::Field { .. } | Place::Index { .. } => unreachable!(),
                }
            }
            StatementKind::Drop(place) => {
                unwrap_infallible(pull_semantics::walk_drop_statement(self, place));
            }
            StatementKind::Unwatch(local) => {
                // Emit unwatch for a watched local going out of scope
                let slot = self.local_slot_or_panic(*local, "Unwatch");
                let inst = self.emit(Instruction::Unwatch(slot));
                self.set_var_operand(inst, slot);
            }
            StatementKind::NotifyBlock { name, level } => {
                // Add block notification to the function's metadata
                let block_index = self.block_notifications.len();
                self.block_notifications.push(BlockNotification {
                    function_name: String::new(), // Filled in by VM at runtime
                    block_name: name.to_string(),
                    level: *level,
                    block_type: BlockNotificationType::Statement,
                    is_enter: true,
                });
                self.emit(Instruction::NotifyBlock(block_index));
            }
            StatementKind::WatchOptions { local, filter } => {
                let channel_name = self.mir.local(*local).name.as_deref();
                unwrap_infallible(pull_semantics::walk_watch_options_statement(
                    self,
                    *local,
                    channel_name,
                    filter,
                ));
            }
            StatementKind::WatchNotify(local) => {
                // Emit manual notify for a watched variable
                let slot = self.local_slot_or_panic(*local, "WatchNotify");
                let inst = self.emit(Instruction::Notify(slot));
                self.set_var_operand(inst, slot);
            }
            StatementKind::VizEnter(node_idx) => {
                self.emit(Instruction::VizEnter(*node_idx));
            }
            StatementKind::VizExit(node_idx) => {
                self.emit(Instruction::VizExit(*node_idx));
            }
            StatementKind::Nop => {}
            StatementKind::Assert(operand) => {
                unwrap_infallible(pull_semantics::walk_assert_statement(self, operand));
            }
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

    /// Emit an rvalue using the pull model.
    fn emit_rvalue_pull(&mut self, rvalue: &Rvalue) {
        unwrap_infallible(pull_semantics::walk_rvalue_pull(self, rvalue));
    }

    /// Emit a constant value.
    fn emit_constant(&mut self, constant: &Constant) {
        match constant {
            Constant::Int(v) => {
                let idx = self.add_constant(ConstValue::Int(*v));
                let inst = self.emit(Instruction::LoadConst(idx));
                self.set_operand(inst, OperandMeta::Const(v.to_string()));
            }
            Constant::Float(v) => {
                let idx = self.add_constant(ConstValue::Float(*v));
                let inst = self.emit(Instruction::LoadConst(idx));
                self.set_operand(inst, OperandMeta::Const(v.to_string()));
            }
            Constant::String(s) => {
                let escaped = s
                    .replace('\\', "\\\\")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t");
                let display = format!("\"{escaped}\"");
                let obj_idx = self.objects.len();
                self.objects.push(Object::String(s.clone()));
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
            Constant::Function(qn) => {
                let name_str = qn.to_runtime_string();
                let global_idx = self
                    .globals
                    .get(&name_str)
                    .unwrap_or_else(|| panic!("undefined function: {name_str}"));
                let inst = self.emit(Instruction::LoadGlobal(GlobalIndex::from_raw(*global_idx)));
                self.set_operand(inst, OperandMeta::Global(format!("<fn {name_str}>")));
            }
            Constant::Ty(_) => {
                let idx = self.add_constant(ConstValue::Null);
                let inst = self.emit(Instruction::LoadConst(idx));
                self.set_operand(inst, OperandMeta::Const("null".to_string()));
            }
            Constant::EnumVariant { enum_qn, variant } => {
                let enum_name_str = enum_qn.to_runtime_string();
                let enum_obj_idx = *self
                    .enum_object_indices
                    .get(&enum_name_str)
                    .unwrap_or_else(|| panic!("undefined enum: {enum_name_str}"));

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
                    OperandMeta::Const(format!(
                        "{variant_idx} /* {enum_name_str}.{variant_str} */"
                    )),
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
                        // Real locals get stored to their slot
                        let slot = self.local_slots[local];
                        let inst = self.emit(Instruction::StoreVar(slot));
                        self.set_var_operand(inst, slot);
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
                // Optimization: If else_block is unreachable (last arm of exhaustive match),
                // we know the condition must be true, so skip the comparison entirely.
                if self.analysis.is_block_unreachable(*else_block, self.mir) {
                    // Don't evaluate condition - just go directly to then_block
                    self.emit_jump_unless_fallthrough(*then_block);
                } else {
                    self.emit_operand_pull(condition);
                    // PopJumpIfFalse to else_block (pops condition from stack)
                    // Apply jump threading to resolve through empty blocks
                    let resolved_else = self.resolve_pending_target(*else_block);
                    let else_jump = self.emit(Instruction::PopJumpIfFalse(0));
                    self.pending_jumps.push((else_jump, resolved_else));
                    // Jump to then_block (may be elided if it's next)
                    self.emit_jump_unless_fallthrough(*then_block);
                }
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
                    .map(GlobalIndex::from_raw);

                if let Some(global_callee) = global_callee {
                    unwrap_infallible(pull_semantics::walk_call_direct_args(self, args));
                    let inst = self.emit(Instruction::Call(global_callee));
                    if let Some(name) = &func_name {
                        self.set_operand(inst, OperandMeta::Callable(name.clone()));
                    }
                } else {
                    unwrap_infallible(pull_semantics::walk_call_indirect_operands(
                        self, callee, args,
                    ));
                    self.emit(Instruction::CallIndirect);
                }
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

            Terminator::DispatchFuture {
                callee,
                args,
                future,
                resume,
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
                            "dispatch_future callee must resolve to a statically-known global function: {callee:?}"
                        )
                    });

                unwrap_infallible(pull_semantics::walk_call_direct_args(self, args));
                let inst = self.emit(Instruction::DispatchFuture(global_callee));
                if let Some(name) = &func_name {
                    self.set_operand(inst, OperandMeta::Callable(name.clone()));
                }
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

            // Update the instruction with the correct default offset
            self.bytecode.instructions[jump_table_pc] = Instruction::JumpTable {
                table_idx: pending.table_idx,
                default: default_offset,
            };

            // Store the completed table
            self.bytecode.jump_tables.push(table);
        }
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
        self.emit_operand_pull(discriminant);

        let num_arms = arms.len();
        for (i, (value, target)) in arms.iter().enumerate() {
            let is_last = i == num_arms - 1;

            // For exhaustive switches, skip the last arm's comparison
            if exhaustive && is_last {
                self.emit(Instruction::Pop(1)); // Pop discriminant
                self.emit_jump_unless_fallthrough(*target);
                return;
            }

            self.emit(Instruction::Copy(0));
            let idx = self.add_constant(ConstValue::Int(*value));
            let inst = self.emit(Instruction::LoadConst(idx));
            let label = name_map
                .get(value)
                .map(|n| (*n).to_string())
                .unwrap_or_else(|| value.to_string());
            self.set_operand(inst, OperandMeta::Const(label));
            self.emit(Instruction::CmpOp(CmpOp::Eq));
            let jump_idx = self.emit(Instruction::PopJumpIfFalse(0));
            self.emit(Instruction::Pop(1));
            self.emit_jump_unless_fallthrough(*target);
            let skip_to = self.current_pc();
            self.patch_jump_to(jump_idx, skip_to);
        }

        self.emit(Instruction::Pop(1));
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

        // 3. Emit JumpTable instruction with placeholder default offset
        let jump_table_pc = self.emit(Instruction::JumpTable {
            table_idx,
            default: 0, // Will be patched later
        });

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
        let label_for = |value: &i64| -> String {
            name_map
                .get(value)
                .map(|n| (*n).to_string())
                .unwrap_or_else(|| value.to_string())
        };

        match arms.len() {
            0 => {
                // No arms left - just fall through to otherwise
                // (already handled by caller)
            }
            1 => {
                // Single arm - emit direct comparison
                let (value, target) = &arms[0];
                self.emit(Instruction::Copy(0));
                let idx = self.add_constant(ConstValue::Int(*value));
                let inst = self.emit(Instruction::LoadConst(idx));
                self.set_operand(inst, OperandMeta::Const(label_for(value)));
                self.emit(Instruction::CmpOp(CmpOp::Eq));
                let jump_idx = self.emit(Instruction::PopJumpIfFalse(0));
                self.emit(Instruction::Pop(1));
                self.emit_jump_unless_fallthrough(*target);
                let skip_to = self.current_pc();
                self.patch_jump_to(jump_idx, skip_to);
            }
            2 => {
                // Two arms - emit both comparisons sequentially
                for (value, target) in arms {
                    self.emit(Instruction::Copy(0));
                    let idx = self.add_constant(ConstValue::Int(*value));
                    let inst = self.emit(Instruction::LoadConst(idx));
                    self.set_operand(inst, OperandMeta::Const(label_for(value)));
                    self.emit(Instruction::CmpOp(CmpOp::Eq));
                    let jump_idx = self.emit(Instruction::PopJumpIfFalse(0));
                    self.emit(Instruction::Pop(1));
                    self.emit_jump_unless_fallthrough(*target);
                    let skip_to = self.current_pc();
                    self.patch_jump_to(jump_idx, skip_to);
                }
            }
            _ => {
                // Multiple arms - split in half and recurse
                let mid = arms.len() / 2;
                let (value, target) = &arms[mid];
                let left = &arms[..mid];
                let right = &arms[mid + 1..];

                // Compare with pivot
                self.emit(Instruction::Copy(0));
                let idx = self.add_constant(ConstValue::Int(*value));
                let inst = self.emit(Instruction::LoadConst(idx));
                self.set_operand(inst, OperandMeta::Const(label_for(value)));
                self.emit(Instruction::CmpOp(CmpOp::Eq));
                let eq_jump = self.emit(Instruction::PopJumpIfFalse(0));

                // If equal, jump to target
                self.emit(Instruction::Pop(1));
                self.emit_jump_unless_fallthrough(*target);
                let after_eq = self.current_pc();
                self.patch_jump_to(eq_jump, after_eq);

                // Compare < pivot for left subtree
                self.emit(Instruction::Copy(0));
                let inst = self.emit(Instruction::LoadConst(idx));
                self.set_operand(inst, OperandMeta::Const(label_for(value)));
                self.emit(Instruction::CmpOp(CmpOp::Lt));
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
            BinOp::Instanceof => Instruction::CmpOp(CmpOp::InstanceOf),
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
    fn build_local_names(mir: &MirFunction, local_slots: &HashMap<Local, usize>) -> Vec<String> {
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
        mir: &MirFunction,
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
                LocalPullAction::Inline(rvalue)
            }
            LocalClassification::PhiLike
            | LocalClassification::ReturnPhi
            | LocalClassification::CallResultImmediate => LocalPullAction::Done,
            LocalClassification::CopyOf => {
                // Copy propagation: load from source slot directly.
                let source = self.analysis.resolve_copy_source(local);
                let slot = self.local_slots[&source];
                let inst = self.emit(Instruction::LoadVar(slot));
                self.set_var_operand(inst, slot);
                LocalPullAction::Done
            }
            LocalClassification::Parameter
            | LocalClassification::Real
            | LocalClassification::Dead => {
                let slot = self.local_slots[&local];
                let inst = self.emit(Instruction::LoadVar(slot));
                self.set_var_operand(inst, slot);
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

    fn alloc_array(&mut self, len: usize) -> Result<(), Self::Error> {
        self.emit(Instruction::AllocArray(len));
        Ok(())
    }

    fn alloc_map(&mut self, len: usize) -> Result<(), Self::Error> {
        self.emit(Instruction::AllocMap(len));
        Ok(())
    }

    fn alloc_class_instance(&mut self, class_name: &str) -> Result<(), Self::Error> {
        let class_obj_idx = self
            .class_object_indices
            .get(class_name)
            .copied()
            .unwrap_or_else(|| panic!("undefined class: {class_name}"));
        let inst = self.emit(Instruction::AllocInstance(ObjectIndex::from_raw(
            class_obj_idx,
        )));
        self.set_operand(inst, OperandMeta::Object(class_name.to_string()));
        Ok(())
    }

    fn copy_top(&mut self, offset: usize) -> Result<(), Self::Error> {
        self.emit(Instruction::Copy(offset));
        Ok(())
    }

    fn store_field(&mut self, field_idx: usize, name: &str) -> Result<(), Self::Error> {
        let idx = self.emit(Instruction::StoreField(field_idx));
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
            OperandMeta::Const(format!("{variant_idx} /* {enum_name}.{variant} */")),
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
        // MIR `Rvalue::Len` is array length.
        let global_idx = self
            .globals
            .get("baml.Array.length")
            .copied()
            .unwrap_or_else(|| panic!("undefined function: baml.Array.length"));
        pull_semantics::walk_place_pull(self, place)?;
        let inst = self.emit(Instruction::Call(GlobalIndex::from_raw(global_idx)));
        self.set_operand(inst, OperandMeta::Callable("baml.Array.length".to_string()));
        Ok(())
    }

    fn is_type(&mut self, ty: &Ty) -> Result<(), Self::Error> {
        // Emit instanceof check using CmpOp::InstanceOf for class aliases.
        if let Ty::Class(tn) | Ty::TypeAlias(tn) = ty {
            let class_name_str = tn.display_name.as_str();
            if let Some(&class_obj_idx) = self.class_object_indices.get(class_name_str) {
                let class_const =
                    self.add_constant(ConstValue::Object(ObjectIndex::from_raw(class_obj_idx)));
                let inst = self.emit(Instruction::LoadConst(class_const));
                self.set_operand(
                    inst,
                    OperandMeta::Const(format!("<class {class_name_str}>")),
                );
                self.emit(Instruction::CmpOp(CmpOp::InstanceOf));
            } else {
                self.emit(Instruction::Pop(1));
                let idx = self.add_constant(ConstValue::Bool(false));
                let inst = self.emit(Instruction::LoadConst(idx));
                self.set_operand(inst, OperandMeta::Const("false".to_string()));
            }
        } else {
            self.emit(Instruction::Pop(1));
            let idx = self.add_constant(ConstValue::Bool(false));
            let inst = self.emit(Instruction::LoadConst(idx));
            self.set_operand(inst, OperandMeta::Const("false".to_string()));
        }
        Ok(())
    }

    fn resolve_field_name(&self, base: &Place, field_idx: usize) -> String {
        let class_name = match self.resolve_place_type(base) {
            Some(Ty::Class(tn)) => tn.display_name.to_string(),
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

    fn push_watch_channel(
        &mut self,
        local: Local,
        channel_name: Option<&str>,
    ) -> Result<(), Self::Error> {
        // Watched locals must be `Real` and therefore must have slots.
        let _slot = self.local_slot_or_panic(local, "WatchOptions/watch initialization");
        let channel = channel_name
            .unwrap_or_else(|| panic!("watched local {local} must have a user-visible name"))
            .to_string();
        let channel_obj_idx = self.objects.len();
        self.objects.push(Object::String(channel.clone()));
        let channel_const_idx =
            self.add_constant(ConstValue::Object(ObjectIndex::from_raw(channel_obj_idx)));
        let inst = self.emit(Instruction::LoadConst(channel_const_idx));
        self.set_operand(inst, OperandMeta::Const(format!("\"{channel}\"")));
        Ok(())
    }

    fn watch_local(&mut self, local: Local) -> Result<(), Self::Error> {
        let slot = self.local_slot_or_panic(local, "Watch");
        let inst = self.emit(Instruction::Watch(slot));
        self.set_var_operand(inst, slot);
        Ok(())
    }

    fn assert_top(&mut self) -> Result<(), Self::Error> {
        self.emit(Instruction::Assert);
        Ok(())
    }
}

// ============================================================================
// Public Entry Point
// ============================================================================

/// Compile a MIR function to bytecode using stackification.
///
/// This is the main entry point for the optimized MIR-based code generation.
pub(crate) fn compile_mir_function<'mir>(
    mir: &'mir MirFunction,
    line_starts: &'mir [u32],
    ctx: MirCodegenContext<'mir, '_>,
    opt: crate::analysis::OptLevel,
) -> Function {
    // Run analysis
    let analysis = AnalysisResult::analyze(mir, opt);
    #[cfg(debug_assertions)]
    crate::verifier::verify_mir_emit_invariants(mir, &analysis);

    // Compile with stackification
    let codegen = StackifyCodegen::new(mir, line_starts, ctx, analysis);
    codegen.compile()
}
