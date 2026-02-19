//! Pull-model bytecode emission with stackification.
//!
//! This module implements the code generation phase that uses the analysis
//! results to emit optimized bytecode. Virtual locals are inlined at their
//! use sites instead of being stored to stack slots.

use std::collections::{HashMap, HashSet};

use baml_compiler_mir::{
    AggregateKind, BasicBlock, BinOp, BlockId, Constant, IndexKind, Local, MirFunction, Operand,
    Place, Rvalue, StatementKind, Terminator, UnaryOp,
};
use baml_type::Ty;
use bex_vm_types::{
    BinOp as VmBinOp, Bytecode, CmpOp, ConstValue, Function, FunctionKind, GlobalIndex,
    Instruction, Object, ObjectIndex, ObjectPool, UnaryOp as VmUnaryOp,
    bytecode::{BlockNotification, BlockNotificationType, JumpTableData},
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
    analysis::{AnalysisResult, LocalClassification},
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
    arms: Vec<(i64, BlockId)>,
    /// Default target block.
    otherwise: BlockId,
    /// The jump table data being built.
    table: JumpTableData,
}

/// MIR to bytecode compiler with stackification.
struct StackifyCodegen<'ctx, 'obj> {
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

    /// Maps `BlockId` -> bytecode instruction index (for jump patching).
    block_addresses: HashMap<BlockId, usize>,

    /// Pending jumps that need patching: (`instruction_index`, `target_block`).
    pending_jumps: Vec<(usize, BlockId)>,

    /// Pending jump tables that need patching after all blocks are emitted.
    pending_jump_tables: Vec<PendingJumpTable>,

    /// Bytecode being generated.
    bytecode: Bytecode,

    /// Current source line for debugging.
    current_source_line: usize,

    /// The next block in RPO order (for fall-through optimization).
    next_block: Option<BlockId>,

    /// Watched locals that have already had Watch instruction emitted.
    /// We only emit Watch once per watched local (at initialization).
    watched_locals_initialized: HashSet<Local>,

    /// Block notifications to be attached to the compiled function.
    block_notifications: Vec<BlockNotification>,
}

impl<'ctx, 'obj> StackifyCodegen<'ctx, 'obj> {
    /// Create a new stackification codegen instance.
    #[allow(clippy::needless_pass_by_value)] // ctx is destructured into self fields
    fn new(ctx: MirCodegenContext<'ctx, 'obj>, analysis: AnalysisResult) -> Self {
        Self {
            globals: ctx.globals,
            classes: ctx.classes,
            class_object_indices: ctx.class_object_indices,
            enum_object_indices: ctx.enum_object_indices,
            enum_variants: ctx.enum_variants,
            objects: ctx.objects,
            analysis,
            local_slots: HashMap::new(),
            block_addresses: HashMap::new(),
            pending_jumps: Vec::new(),
            pending_jump_tables: Vec::new(),
            bytecode: Bytecode::new(),
            current_source_line: 0,
            next_block: None,
            watched_locals_initialized: HashSet::new(),
            block_notifications: Vec::new(),
        }
    }

    /// Compile a MIR function to bytecode.
    fn compile(mut self, mir: &MirFunction) -> Function {
        // 1. Allocate stack slots only for real locals
        self.allocate_real_locals(mir);

        // 2. Emit blocks in RPO order
        let rpo = self.analysis.rpo.clone();
        for (i, &block_id) in rpo.iter().enumerate() {
            self.block_addresses.insert(block_id, self.current_pc());
            // Track the next block for fall-through optimization
            self.next_block = rpo.get(i + 1).copied();

            // Skip emitting dead unreachable blocks - they're targets for impossible
            // control flow paths (e.g., exhaustive match fallthrough). We record
            // their address (current PC) so jumps to them resolve, but don't emit
            // any instructions. If somehow reached, execution falls through to
            // whatever comes next.
            let block = mir.block(block_id);
            if crate::analysis::is_dead_unreachable_block(block) {
                continue;
            }

            self.emit_block(block, mir);
        }

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

        // 5. Build the Function
        Function {
            name: mir.name.to_string(),
            arity: mir.arity,
            bytecode: self.bytecode,
            kind: FunctionKind::Bytecode,
            locals_in_scope: Self::build_locals_in_scope(mir, &self.local_slots),
            span: baml_base::Span::fake(),
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

        // Pre-allocate only the real locals (not virtuals)
        if slots_to_allocate > 0 {
            self.emit(Instruction::InitLocals(slots_to_allocate));
        }
    }

    /// Get current program counter (next instruction index).
    fn current_pc(&self) -> usize {
        self.bytecode.instructions.len()
    }

    /// Emit an instruction and return its index.
    fn emit(&mut self, instruction: Instruction) -> usize {
        let index = self.bytecode.instructions.len();
        self.bytecode.instructions.push(instruction);
        self.bytecode.source_lines.push(self.current_source_line);
        self.bytecode.scopes.push(0);
        index
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
        // Apply jump threading: resolve through redirect map
        let resolved_target = self.analysis.resolve_jump_target(target);

        // Check if we can fall through:
        // 1. Next block IS the resolved target, OR
        // 2. Next block is an empty block that resolves to our target
        //    (fall through to it, and it will take us there)
        let can_fall_through = self.next_block.is_some_and(|next| {
            let resolved_next = self.analysis.resolve_jump_target(next);
            resolved_target == next || resolved_target == resolved_next
        });

        if can_fall_through {
            // No jump needed - fall through will get us there
            false
        } else {
            let jump_idx = self.emit(Instruction::Jump(0));
            self.pending_jumps.push((jump_idx, resolved_target));
            true
        }
    }

    // ========================================================================
    // Block Emission
    // ========================================================================

    /// Emit a basic block.
    fn emit_block(&mut self, block: &BasicBlock, mir: &MirFunction) {
        // Emit all statements
        for stmt in &block.statements {
            self.emit_statement(&stmt.kind, mir);
        }

        // Emit terminator
        if let Some(term) = &block.terminator {
            self.emit_terminator(term, mir);
        }
    }

    /// Emit a statement (with virtual assignment skipping).
    fn emit_statement(&mut self, kind: &StatementKind, mir: &MirFunction) {
        match kind {
            StatementKind::Assign { destination, value } => {
                // Check if this is an assignment to a Virtual, PhiLike, or Dead local
                if let Place::Local(local) = destination {
                    match self.analysis.classifications[local] {
                        LocalClassification::Virtual => {
                            // Skip! This will be inlined at use site
                            return;
                        }
                        LocalClassification::PhiLike | LocalClassification::ReturnPhi => {
                            // Emit rvalue (leaves value on stack) but NOT the store.
                            // PhiLike: value stays on stack until the join point uses it.
                            // ReturnPhi: value stays on stack until Return.
                            self.emit_rvalue_pull(value, mir);
                            return;
                        }
                        LocalClassification::CopyOf => {
                            // Copy propagation - skip the copy entirely.
                            // Uses of this local will load from the source instead.
                            return;
                        }
                        LocalClassification::Dead => {
                            // Dead store elimination - skip entirely
                            return;
                        }
                        _ => {}
                    }
                }

                // For field/index stores, push the base object first, then emit the value
                // This sets up the stack correctly for StoreField/StoreArrayElement
                match destination {
                    Place::Field { base, field } => {
                        // For field store: push object, then value, then StoreField
                        self.emit_place_value_pull(base, mir); // push object
                        self.emit_rvalue_pull(value, mir); // push value
                        self.emit(Instruction::StoreField(*field));
                    }
                    Place::Index { base, index, kind } => {
                        // For index store: push array/map, then index/key, then value, then Store*Element
                        self.emit_place_value_pull(base, mir); // push array/map
                        self.emit_place_value_pull(&Place::Local(*index), mir); // push index/key
                        self.emit_rvalue_pull(value, mir); // push value
                        match kind {
                            IndexKind::Array => self.emit(Instruction::StoreArrayElement),
                            IndexKind::Map => self.emit(Instruction::StoreMapElement),
                        };
                    }
                    Place::Local(local) => {
                        // Local assignment: emit rvalue then store
                        self.emit_rvalue_pull(value, mir);
                        self.emit_store_place(destination, mir);
                        // Emit Watch only once for watched locals (at initialization)
                        let local_decl = mir.local(*local);
                        if local_decl.is_watched && !self.watched_locals_initialized.contains(local)
                        {
                            self.watched_locals_initialized.insert(*local);
                            if let Some(&slot) = self.local_slots.get(local) {
                                // Push channel name (variable name) and filter (null for default)
                                let channel =
                                    local_decl.name.as_ref().map_or("_watch", |n| n.as_str());
                                let channel_obj_idx = self.objects.len();
                                self.objects.push(Object::String(channel.to_string()));
                                let channel_const_idx = self.add_constant(ConstValue::Object(
                                    ObjectIndex::from_raw(channel_obj_idx),
                                ));
                                self.emit(Instruction::LoadConst(channel_const_idx));
                                let null_const_idx = self.add_constant(ConstValue::Null);
                                self.emit(Instruction::LoadConst(null_const_idx));
                                self.emit(Instruction::Watch(slot));
                            }
                        }
                    }
                }
            }
            StatementKind::Drop(place) => {
                self.emit_place_value_pull(place, mir);
                self.emit(Instruction::Pop(1));
            }
            StatementKind::Unwatch(local) => {
                // Emit unwatch for a watched local going out of scope
                if let Some(&slot) = self.local_slots.get(local) {
                    self.emit(Instruction::Unwatch(slot));
                }
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
                // Emit Watch instruction with new filter
                // This updates the watch settings for an already-watched variable
                if let Some(&slot) = self.local_slots.get(local) {
                    let local_decl = mir.local(*local);
                    // Push channel name
                    let channel = local_decl.name.as_ref().map_or("_watch", |n| n.as_str());
                    let channel_obj_idx = self.objects.len();
                    self.objects.push(Object::String(channel.to_string()));
                    let channel_const_idx = self
                        .add_constant(ConstValue::Object(ObjectIndex::from_raw(channel_obj_idx)));
                    self.emit(Instruction::LoadConst(channel_const_idx));
                    // Push filter value
                    self.emit_operand_pull(filter, mir);
                    // Re-emit Watch with new filter
                    self.emit(Instruction::Watch(slot));
                }
            }
            StatementKind::WatchNotify(local) => {
                // Emit manual notify for a watched variable
                if let Some(&slot) = self.local_slots.get(local) {
                    self.emit(Instruction::Notify(slot));
                }
            }
            StatementKind::VizEnter(node_idx) => {
                self.emit(Instruction::VizEnter(*node_idx));
            }
            StatementKind::VizExit(node_idx) => {
                self.emit(Instruction::VizExit(*node_idx));
            }
            StatementKind::Nop => {}
            StatementKind::Assert(operand) => {
                self.emit_operand_pull(operand, mir);
                self.emit(Instruction::Assert);
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
    fn emit_operand_pull(&mut self, operand: &Operand, mir: &MirFunction) {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => {
                self.emit_place_value_pull(place, mir);
            }
            Operand::Constant(constant) => {
                self.emit_constant(constant);
            }
        }
    }

    /// Emit a place's value using the pull model.
    fn emit_place_value_pull(&mut self, place: &Place, mir: &MirFunction) {
        match place {
            Place::Local(local) => {
                let classification = self.analysis.classifications[local];

                match classification {
                    LocalClassification::Virtual => {
                        // PULL: emit the definition's rvalue inline
                        // Clone the rvalue to avoid borrow checker issues
                        let rvalue = self.analysis.def_use[local]
                            .def
                            .as_ref()
                            .map(|def| def.rvalue.clone())
                            .unwrap_or_else(|| panic!("virtual local {local} without definition"));
                        self.emit_rvalue_pull(&rvalue, mir);
                    }
                    LocalClassification::PhiLike
                    | LocalClassification::ReturnPhi
                    | LocalClassification::CallResultImmediate => {
                        // PhiLike: value is already on the stack from the predecessor block.
                        // ReturnPhi: value is already on the stack from the assignment.
                        // CallResultImmediate: value is already on the stack from the Call.
                        // Don't emit any instruction - the value is there waiting for us.
                    }
                    LocalClassification::CopyOf => {
                        // Copy propagation: load from the source local instead.
                        let source = self.analysis.resolve_copy_source(*local);
                        let slot = self.local_slots[&source];
                        self.emit(Instruction::LoadVar(slot));
                    }
                    _ => {
                        // Real/Parameter local: emit LoadVar
                        let slot = self.local_slots[local];
                        self.emit(Instruction::LoadVar(slot));
                    }
                }
            }
            Place::Field { base, field } => {
                // Load base then field
                self.emit_place_value_pull(base, mir);
                self.emit(Instruction::LoadField(*field));
            }
            Place::Index { base, index, kind } => {
                // Load base, load index, then LoadArrayElement or LoadMapElement
                self.emit_place_value_pull(base, mir);
                // Index may be virtual or real
                self.emit_place_value_pull(&Place::Local(*index), mir);
                match kind {
                    IndexKind::Array => self.emit(Instruction::LoadArrayElement),
                    IndexKind::Map => self.emit(Instruction::LoadMapElement),
                };
            }
        }
    }

    /// Emit an rvalue using the pull model.
    fn emit_rvalue_pull(&mut self, rvalue: &Rvalue, mir: &MirFunction) {
        match rvalue {
            Rvalue::Use(operand) => {
                self.emit_operand_pull(operand, mir);
            }

            Rvalue::BinaryOp { op, left, right } => {
                self.emit_operand_pull(left, mir);
                self.emit_operand_pull(right, mir);
                self.emit(Self::binop_instruction(*op));
            }

            Rvalue::UnaryOp { op, operand } => {
                self.emit_operand_pull(operand, mir);
                self.emit(Self::unaryop_instruction(*op));
            }

            Rvalue::Array(elements) => {
                for elem in elements {
                    self.emit_operand_pull(elem, mir);
                }
                self.emit(Instruction::AllocArray(elements.len()));
            }

            Rvalue::Map(entries) => {
                // For maps, VM expects stack layout: [value1, value2, ..., valueN, key1, key2, ..., keyN]
                // Push all values first
                for (_key, value) in entries {
                    self.emit_operand_pull(value, mir);
                }
                // Then push all keys
                for (key, _value) in entries {
                    self.emit_operand_pull(key, mir);
                }
                self.emit(Instruction::AllocMap(entries.len()));
            }

            Rvalue::Aggregate { kind, fields } => {
                match kind {
                    AggregateKind::Array => {
                        for field in fields {
                            self.emit_operand_pull(field, mir);
                        }
                        self.emit(Instruction::AllocArray(fields.len()));
                    }
                    AggregateKind::Class(class_name) => {
                        // Look up pre-allocated Class object index
                        let class_obj_idx = self
                            .class_object_indices
                            .get(class_name)
                            .copied()
                            .unwrap_or_else(|| panic!("undefined class: {class_name}"));

                        // Emit AllocInstance
                        self.emit(Instruction::AllocInstance(ObjectIndex::from_raw(
                            class_obj_idx,
                        )));

                        // For each field: Copy instance, emit field value, StoreField
                        for (field_idx, field_operand) in fields.iter().enumerate() {
                            self.emit(Instruction::Copy(0));
                            self.emit_operand_pull(field_operand, mir);
                            self.emit(Instruction::StoreField(field_idx));
                        }
                    }
                    AggregateKind::EnumVariant { enum_name, variant } => {
                        // Look up the enum object index
                        let enum_obj_idx = self
                            .enum_object_indices
                            .get(enum_name)
                            .copied()
                            .unwrap_or_else(|| panic!("undefined enum: {enum_name}"));

                        // Look up the variant index
                        let variant_idx = self
                            .enum_variants
                            .get(enum_name)
                            .and_then(|variants| variants.get(variant))
                            .copied()
                            .unwrap_or_else(|| panic!("undefined variant: {enum_name}.{variant}"));

                        // Load variant index onto stack, then allocate variant
                        #[allow(clippy::cast_possible_wrap)]
                        let idx = self.add_constant(ConstValue::Int(variant_idx as i64));
                        self.emit(Instruction::LoadConst(idx));
                        self.emit(Instruction::AllocVariant(ObjectIndex::from_raw(
                            enum_obj_idx,
                        )));
                    }
                }
            }

            Rvalue::Discriminant(place) => {
                self.emit_place_value_pull(place, mir);
                self.emit(Instruction::Discriminant);
            }

            Rvalue::TypeTag(place) => {
                self.emit_place_value_pull(place, mir);
                self.emit(Instruction::TypeTag);
            }

            Rvalue::Len(place) => {
                self.emit_place_value_pull(place, mir);
                // TODO: Proper length builtin call
                if let Some(&global_idx) = self.globals.get("baml.Array.length") {
                    self.emit(Instruction::LoadGlobal(GlobalIndex::from_raw(global_idx)));
                    // Stack ordering issue - same as original codegen
                }
            }

            Rvalue::IsType { operand, ty } => {
                self.emit_operand_pull(operand, mir);
                // Emit instanceof check using CmpOp::InstanceOf
                // The type should be a class name - look up the class object
                if let Ty::Class(tn) | Ty::TypeAlias(tn) = ty {
                    let class_name_str = tn.display_name.as_str();
                    if let Some(&class_obj_idx) = self.class_object_indices.get(class_name_str) {
                        // Load the Class object for the type check
                        let class_const = self
                            .add_constant(ConstValue::Object(ObjectIndex::from_raw(class_obj_idx)));
                        self.emit(Instruction::LoadConst(class_const));
                        // Emit instanceof comparison
                        self.emit(Instruction::CmpOp(CmpOp::InstanceOf));
                    } else {
                        // Unknown class - treat as always false
                        let idx = self.add_constant(ConstValue::Bool(false));
                        self.emit(Instruction::LoadConst(idx));
                    }
                } else {
                    // Non-class type - not supported yet, return false
                    let idx = self.add_constant(ConstValue::Bool(false));
                    self.emit(Instruction::LoadConst(idx));
                }
            }
        }
    }

    /// Emit a constant value.
    fn emit_constant(&mut self, constant: &Constant) {
        match constant {
            Constant::Int(v) => {
                let idx = self.add_constant(ConstValue::Int(*v));
                self.emit(Instruction::LoadConst(idx));
            }
            Constant::Float(v) => {
                let idx = self.add_constant(ConstValue::Float(*v));
                self.emit(Instruction::LoadConst(idx));
            }
            Constant::String(s) => {
                let obj_idx = self.objects.len();
                self.objects.push(Object::String(s.clone()));
                let idx = self.add_constant(ConstValue::Object(ObjectIndex::from_raw(obj_idx)));
                self.emit(Instruction::LoadConst(idx));
            }
            Constant::Bool(v) => {
                let idx = self.add_constant(ConstValue::Bool(*v));
                self.emit(Instruction::LoadConst(idx));
            }
            Constant::Null => {
                let idx = self.add_constant(ConstValue::Null);
                self.emit(Instruction::LoadConst(idx));
            }
            Constant::Function(qn) => {
                // Convert QualifiedName to runtime string for function lookup
                let name_str = qn.to_runtime_string();
                let global_idx = self
                    .globals
                    .get(&name_str)
                    .unwrap_or_else(|| panic!("undefined function: {name_str}"));
                self.emit(Instruction::LoadGlobal(GlobalIndex::from_raw(*global_idx)));
            }
            Constant::Ty(_) => {
                let idx = self.add_constant(ConstValue::Null);
                self.emit(Instruction::LoadConst(idx));
            }
            Constant::EnumVariant { enum_qn, variant } => {
                // Look up the enum object index
                // Convert QualifiedName to runtime string for lookup
                let enum_name_str = enum_qn.to_runtime_string();
                let enum_obj_idx = *self
                    .enum_object_indices
                    .get(&enum_name_str)
                    .unwrap_or_else(|| panic!("undefined enum: {enum_name_str}"));

                // Look up the variant index
                let variant_str = variant.to_string();
                let variant_idx = *self
                    .enum_variants
                    .get(&enum_name_str)
                    .and_then(|variants| variants.get(&variant_str))
                    .unwrap_or_else(|| panic!("undefined variant: {enum_name_str}.{variant_str}"));

                // Load variant index onto stack, then allocate variant
                #[allow(clippy::cast_possible_wrap)]
                let idx = self.add_constant(ConstValue::Int(variant_idx as i64));
                self.emit(Instruction::LoadConst(idx));
                self.emit(Instruction::AllocVariant(ObjectIndex::from_raw(
                    enum_obj_idx,
                )));
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
    fn emit_store_place(&mut self, place: &Place, _mir: &MirFunction) {
        match place {
            Place::Local(local) => {
                let classification = self.analysis.classifications[local];
                match classification {
                    LocalClassification::Parameter | LocalClassification::Real => {
                        // Real locals get stored to their slot
                        let slot = self.local_slots[local];
                        self.emit(Instruction::StoreVar(slot));
                    }
                    LocalClassification::PhiLike
                    | LocalClassification::ReturnPhi
                    | LocalClassification::CallResultImmediate => {
                        // PhiLike/ReturnPhi: keep value on stack (no-op) - value goes to join/return.
                        // CallResultImmediate: keep value on stack (no-op) - value used immediately.
                    }
                    LocalClassification::Virtual
                    | LocalClassification::CopyOf
                    | LocalClassification::Dead => {
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
    fn emit_terminator(&mut self, term: &Terminator, mir: &MirFunction) {
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
                if self.analysis.is_block_unreachable(*else_block, mir) {
                    // Don't evaluate condition - just go directly to then_block
                    self.emit_jump_unless_fallthrough(*then_block);
                } else {
                    self.emit_operand_pull(condition, mir);
                    // PopJumpIfFalse to else_block (pops condition from stack)
                    // Apply jump threading to resolve through empty blocks
                    let resolved_else = self.analysis.resolve_jump_target(*else_block);
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
            } => {
                // Analyze the switch to determine the best emission strategy
                let strategy = analyze_switch(arms);

                match strategy {
                    SwitchStrategy::JumpTable { min, max } => {
                        self.emit_switch_jump_table(discriminant, arms, *otherwise, min, max, mir);
                    }
                    SwitchStrategy::BinarySearch => {
                        self.emit_switch_binary_search(
                            discriminant,
                            arms,
                            *otherwise,
                            *exhaustive,
                            mir,
                        );
                    }
                    SwitchStrategy::IfElseChain => {
                        self.emit_switch_if_else(discriminant, arms, *otherwise, *exhaustive, mir);
                    }
                }
            }

            Terminator::Return => {
                // Use pull model for return value - if _0 is Virtual, inline it
                self.emit_place_value_pull(&Place::Local(Local(0)), mir);
                self.emit(Instruction::Return);
            }

            Terminator::Call {
                callee,
                args,
                destination,
                target,
                unwind: _,
            } => {
                self.emit_operand_pull(callee, mir);
                for arg in args {
                    self.emit_operand_pull(arg, mir);
                }
                self.emit(Instruction::Call(args.len()));
                self.emit_store_place(destination, mir);
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
                self.emit_operand_pull(callee, mir);
                for arg in args {
                    self.emit_operand_pull(arg, mir);
                }
                self.emit(Instruction::DispatchFuture(args.len()));
                self.emit_store_place(future, mir);
                self.emit_jump_unless_fallthrough(*resume);
            }

            Terminator::Await {
                future,
                destination,
                target,
                unwind: _,
            } => {
                self.emit_place_value_pull(future, mir);
                self.emit(Instruction::Await);
                self.emit_store_place(destination, mir);
                self.emit_jump_unless_fallthrough(*target);
            }
        }
    }

    // ========================================================================
    // Jump Patching
    // ========================================================================

    /// Patch all pending jumps with actual addresses.
    fn patch_jumps(&mut self) {
        for (instruction_idx, target_block) in self.pending_jumps.clone() {
            let target_pc = self.block_addresses[&target_block];
            self.patch_jump_to(instruction_idx, target_pc);
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
                let target_pc = self.block_addresses[target];
                let offset = target_pc as isize - jump_table_pc as isize;
                table.set(*value, offset);
            }

            // Patch default offset
            let otherwise_pc = self.block_addresses[&pending.otherwise];
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
        mir: &MirFunction,
    ) {
        self.emit_operand_pull(discriminant, mir);

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
            self.emit(Instruction::LoadConst(idx));
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
        mir: &MirFunction,
    ) {
        // 1. Push discriminant onto stack
        self.emit_operand_pull(discriminant, mir);

        // 2. Create jump table data structure with placeholder offsets
        let table_idx = self.pending_jump_tables.len();
        let table = JumpTableData::new(min, max);

        // 3. Emit JumpTable instruction with placeholder default offset
        let jump_table_pc = self.emit(Instruction::JumpTable {
            table_idx,
            default: 0, // Will be patched later
        });

        // 4. Record pending jump table for patching
        self.pending_jump_tables.push(PendingJumpTable {
            table_idx,
            jump_table_pc,
            arms: arms.to_vec(),
            otherwise,
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
        mir: &MirFunction,
    ) {
        // Push discriminant onto stack (will be popped by comparisons)
        self.emit_operand_pull(discriminant, mir);

        // Sort arms by value for binary search
        let mut sorted_arms: Vec<_> = arms.to_vec();
        sorted_arms.sort_by_key(|(v, _)| *v);

        // Emit binary search tree
        self.emit_binary_search_node(&sorted_arms, otherwise);

        // Pop the discriminant if we fall through (shouldn't happen with well-formed switches)
        self.emit(Instruction::Pop(1));
        self.emit_jump_unless_fallthrough(otherwise);
    }

    /// Recursively emit a binary search node.
    ///
    /// The discriminant is already on the stack. We emit comparisons to split
    /// the search space in half at each level.
    #[allow(clippy::only_used_in_recursion)]
    fn emit_binary_search_node(&mut self, arms: &[(i64, BlockId)], otherwise: BlockId) {
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
                self.emit(Instruction::LoadConst(idx));
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
                    self.emit(Instruction::LoadConst(idx));
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
                self.emit(Instruction::LoadConst(idx));
                self.emit(Instruction::CmpOp(CmpOp::Eq));
                let eq_jump = self.emit(Instruction::PopJumpIfFalse(0));

                // If equal, jump to target
                self.emit(Instruction::Pop(1));
                self.emit_jump_unless_fallthrough(*target);
                let after_eq = self.current_pc();
                self.patch_jump_to(eq_jump, after_eq);

                // Compare < pivot for left subtree
                self.emit(Instruction::Copy(0));
                self.emit(Instruction::LoadConst(idx));
                self.emit(Instruction::CmpOp(CmpOp::Lt));
                let lt_jump = self.emit(Instruction::PopJumpIfFalse(0));

                // Left subtree (values < pivot)
                self.emit_binary_search_node(left, otherwise);

                let after_left = self.current_pc();
                self.patch_jump_to(lt_jump, after_left);

                // Right subtree (values > pivot)
                self.emit_binary_search_node(right, otherwise);
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

    /// Build `locals_in_scope` debug info from MIR and actual slot assignments.
    ///
    /// This ensures user variable names are preserved in bytecode output,
    /// mapping slot indices to their actual names based on how locals were assigned.
    fn build_locals_in_scope(
        mir: &MirFunction,
        local_slots: &HashMap<Local, usize>,
    ) -> Vec<Vec<String>> {
        // Find the maximum slot index to size the names vector
        let max_slot = local_slots.values().max().copied().unwrap_or(0);

        // Initialize with placeholder names (slot 0 is function reference)
        let mut names = vec![String::new(); max_slot + 1];
        names[0] = format!("<fn {}>", mir.name);

        // Fill in actual names based on slot assignments
        for (&local, &slot) in local_slots {
            let local_decl = mir.local(local);
            let name = local_decl
                .name
                .as_ref()
                .map(std::string::ToString::to_string)
                .unwrap_or_else(|| format!("_{}", local.0));
            names[slot] = name;
        }

        vec![names]
    }
}

// ============================================================================
// Public Entry Point
// ============================================================================

/// Compile a MIR function to bytecode using stackification.
///
/// This is the main entry point for the optimized MIR-based code generation.
pub(crate) fn compile_mir_function(mir: &MirFunction, ctx: MirCodegenContext<'_, '_>) -> Function {
    // Run analysis
    let analysis = AnalysisResult::analyze(mir);

    // Compile with stackification
    let codegen = StackifyCodegen::new(ctx, analysis);
    codegen.compile(mir)
}
