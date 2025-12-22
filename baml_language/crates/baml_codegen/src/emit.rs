//! Pull-model bytecode emission with stackification.
//!
//! This module implements the code generation phase that uses the analysis
//! results to emit optimized bytecode. Virtual locals are inlined at their
//! use sites instead of being stored to stack slots.

use std::collections::HashMap;

use baml_mir::{
    AggregateKind, BasicBlock, BinOp, BlockId, Constant, Local, MirFunction, Operand, Place,
    Rvalue, StatementKind, Terminator, UnaryOp,
};
use baml_vm::{
    BinOp as VmBinOp, Bytecode, CmpOp, Function, FunctionKind, GlobalIndex, Instruction, Object,
    ObjectIndex, ObjectPool, UnaryOp as VmUnaryOp, Value,
};

use crate::{
    MirCodegenContext,
    analysis::{AnalysisResult, LocalClassification},
};

// ============================================================================
// Stackification Codegen
// ============================================================================

/// MIR to bytecode compiler with stackification.
struct StackifyCodegen<'ctx, 'obj, 'db> {
    /// Resolved global names to indices.
    globals: &'ctx HashMap<String, usize>,
    /// Resolved class field indices.
    #[allow(dead_code)]
    classes: &'ctx HashMap<String, HashMap<String, usize>>,
    /// Pre-allocated Class object indices.
    class_object_indices: &'ctx HashMap<String, usize>,
    /// Shared object pool.
    objects: &'obj mut ObjectPool,

    /// Analysis results (classifications, def-use, etc.).
    analysis: AnalysisResult<'db>,

    /// Maps MIR Local -> stack slot index (only for Real locals).
    local_slots: HashMap<Local, usize>,

    /// Maps `BlockId` -> bytecode instruction index (for jump patching).
    block_addresses: HashMap<BlockId, usize>,

    /// Pending jumps that need patching: (`instruction_index`, `target_block`).
    pending_jumps: Vec<(usize, BlockId)>,

    /// Bytecode being generated.
    bytecode: Bytecode,

    /// Current source line for debugging.
    current_source_line: usize,

    /// The next block in RPO order (for fall-through optimization).
    next_block: Option<BlockId>,
}

impl<'ctx, 'obj, 'db> StackifyCodegen<'ctx, 'obj, 'db> {
    /// Create a new stackification codegen instance.
    #[allow(clippy::needless_pass_by_value)] // ctx is destructured into self fields
    fn new(ctx: MirCodegenContext<'ctx, 'obj>, analysis: AnalysisResult<'db>) -> Self {
        Self {
            globals: ctx.globals,
            classes: ctx.classes,
            class_object_indices: ctx.class_object_indices,
            objects: ctx.objects,
            analysis,
            local_slots: HashMap::new(),
            block_addresses: HashMap::new(),
            pending_jumps: Vec::new(),
            bytecode: Bytecode::new(),
            current_source_line: 0,
            next_block: None,
        }
    }

    /// Compile a MIR function to bytecode.
    fn compile(mut self, mir: &MirFunction<'db>) -> Function {
        // 1. Allocate stack slots only for real locals
        self.allocate_real_locals(mir);

        // 2. Emit blocks in RPO order
        let rpo = self.analysis.rpo.clone();
        for (i, &block_id) in rpo.iter().enumerate() {
            self.block_addresses.insert(block_id, self.current_pc());
            // Track the next block for fall-through optimization
            self.next_block = rpo.get(i + 1).copied();
            self.emit_block(mir.block(block_id), mir);
        }

        // 3. Patch all jump targets
        self.patch_jumps();

        // 4. Build the Function
        Function {
            name: mir.name.clone(),
            arity: mir.arity,
            bytecode: self.bytecode,
            kind: FunctionKind::Exec,
            locals_in_scope: Self::build_locals_in_scope(mir, &self.local_slots),
            span: baml_base::Span::fake(),
            block_notifications: Vec::new(),
        }
    }

    /// Allocate stack slots only for Real locals.
    ///
    /// Virtual locals don't get slots - they're inlined at use sites.
    fn allocate_real_locals(&mut self, mir: &MirFunction<'_>) {
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
                LocalClassification::Virtual => {
                    // Virtual locals don't get slots!
                }
            }
        }

        // Pre-allocate only the real locals (not virtuals)
        if slots_to_allocate > 0 {
            let null_idx = self.add_constant(Value::Null);
            for _ in 0..slots_to_allocate {
                self.emit(Instruction::LoadConst(null_idx));
            }
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
    fn add_constant(&mut self, value: Value) -> usize {
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
    /// Returns true if a jump was emitted, false if it was elided.
    fn emit_jump_unless_fallthrough(&mut self, target: BlockId) -> bool {
        if self.next_block == Some(target) {
            // Target is the next block - no jump needed, just fall through
            false
        } else {
            let jump_idx = self.emit(Instruction::Jump(0));
            self.pending_jumps.push((jump_idx, target));
            true
        }
    }

    // ========================================================================
    // Block Emission
    // ========================================================================

    /// Emit a basic block.
    fn emit_block(&mut self, block: &BasicBlock<'db>, mir: &MirFunction<'db>) {
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
    fn emit_statement(&mut self, kind: &StatementKind<'db>, mir: &MirFunction<'db>) {
        match kind {
            StatementKind::Assign { destination, value } => {
                // Check if this is an assignment to a Virtual local
                if let Place::Local(local) = destination {
                    if self.analysis.classifications[local] == LocalClassification::Virtual {
                        // Skip! This will be inlined at use site
                        return;
                    }
                }

                // Real assignment: emit rvalue then store
                self.emit_rvalue_pull(value, mir);
                self.emit_store_place(destination, mir);
            }
            StatementKind::Drop(place) => {
                self.emit_place_value_pull(place, mir);
                self.emit(Instruction::Pop(1));
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
    fn emit_operand_pull(&mut self, operand: &Operand<'db>, mir: &MirFunction<'db>) {
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
    fn emit_place_value_pull(&mut self, place: &Place, mir: &MirFunction<'db>) {
        match place {
            Place::Local(local) => {
                let classification = self.analysis.classifications[local];

                if classification == LocalClassification::Virtual {
                    // PULL: emit the definition's rvalue inline
                    // Clone the rvalue to avoid borrow checker issues
                    let rvalue = self.analysis.def_use[local]
                        .def
                        .as_ref()
                        .map(|def| def.rvalue.clone())
                        .unwrap_or_else(|| panic!("virtual local {local} without definition"));
                    self.emit_rvalue_pull(&rvalue, mir);
                } else {
                    // Real local: emit LoadVar
                    let slot = self.local_slots[local];
                    self.emit(Instruction::LoadVar(slot));
                }
            }
            Place::Field { base, field } => {
                // Load base then field
                self.emit_place_value_pull(base, mir);
                self.emit(Instruction::LoadField(*field));
            }
            Place::Index { base, index } => {
                // Load base, load index, then LoadArrayElement
                self.emit_place_value_pull(base, mir);
                // Index may be virtual or real
                self.emit_place_value_pull(&Place::Local(*index), mir);
                self.emit(Instruction::LoadArrayElement);
            }
        }
    }

    /// Emit an rvalue using the pull model.
    fn emit_rvalue_pull(&mut self, rvalue: &Rvalue<'db>, mir: &MirFunction<'db>) {
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
                    AggregateKind::EnumVariant { .. } => {
                        // TODO: Implement enum variant construction
                        let idx = self.add_constant(Value::Null);
                        self.emit(Instruction::LoadConst(idx));
                    }
                }
            }

            Rvalue::Discriminant(place) => {
                self.emit_place_value_pull(place, mir);
                // TODO: Emit actual discriminant extraction instruction
            }

            Rvalue::Len(place) => {
                self.emit_place_value_pull(place, mir);
                // TODO: Proper length builtin call
                if let Some(&global_idx) = self.globals.get("baml.Array.length") {
                    self.emit(Instruction::LoadGlobal(GlobalIndex::from_raw(global_idx)));
                    // Stack ordering issue - same as original codegen
                }
            }
        }
    }

    /// Emit a constant value.
    fn emit_constant(&mut self, constant: &Constant<'db>) {
        match constant {
            Constant::Int(v) => {
                let idx = self.add_constant(Value::Int(*v));
                self.emit(Instruction::LoadConst(idx));
            }
            Constant::Float(v) => {
                let idx = self.add_constant(Value::Float(*v));
                self.emit(Instruction::LoadConst(idx));
            }
            Constant::String(s) => {
                let obj_idx = self.objects.len();
                self.objects.push(Object::String(s.clone()));
                let idx = self.add_constant(Value::Object(ObjectIndex::from_raw(obj_idx)));
                self.emit(Instruction::LoadConst(idx));
            }
            Constant::Bool(v) => {
                let idx = self.add_constant(Value::Bool(*v));
                self.emit(Instruction::LoadConst(idx));
            }
            Constant::Null => {
                let idx = self.add_constant(Value::Null);
                self.emit(Instruction::LoadConst(idx));
            }
            Constant::Function(name) => {
                let name_str = name.to_string();
                if let Some(&global_idx) = self.globals.get(&name_str) {
                    self.emit(Instruction::LoadGlobal(GlobalIndex::from_raw(global_idx)));
                } else {
                    panic!("undefined function: {name_str}");
                }
            }
            Constant::Ty(_) => {
                let idx = self.add_constant(Value::Null);
                self.emit(Instruction::LoadConst(idx));
            }
        }
    }

    // ========================================================================
    // Store Emission
    // ========================================================================

    /// Emit code to store the top-of-stack value to a place.
    fn emit_store_place(&mut self, place: &Place, mir: &MirFunction<'db>) {
        match place {
            Place::Local(local) => {
                let slot = self.local_slots[local];
                self.emit(Instruction::StoreVar(slot));
            }
            Place::Field { base, field } => {
                // Stack has: [value]
                // Need: [object, value] for StoreField
                if let Place::Local(base_local) = base.as_ref() {
                    // Swap workaround using _0's slot as temp
                    let temp_slot = self.local_slots[&Local(0)];
                    self.emit(Instruction::StoreVar(temp_slot)); // pop value to temp
                    let base_slot = self.local_slots[base_local];
                    self.emit(Instruction::LoadVar(base_slot)); // push object
                    self.emit(Instruction::LoadVar(temp_slot)); // push value back
                    self.emit(Instruction::StoreField(*field));
                } else {
                    // Complex nested field store - not fully supported yet
                    self.emit(Instruction::Pop(1));
                }
            }
            Place::Index { base, index } => {
                // StoreArrayElement expects: [array, index, value]
                // We have: [value]
                if let Place::Local(base_local) = base.as_ref() {
                    let temp_slot = self.local_slots[&Local(0)];
                    self.emit(Instruction::StoreVar(temp_slot)); // save value
                    let base_slot = self.local_slots[base_local];
                    self.emit(Instruction::LoadVar(base_slot)); // push array
                    // Index may be virtual or real - use pull model
                    self.emit_place_value_pull(&Place::Local(*index), mir);
                    self.emit(Instruction::LoadVar(temp_slot)); // push value
                    self.emit(Instruction::StoreArrayElement);
                } else {
                    self.emit(Instruction::Pop(1));
                }
            }
        }
    }

    // ========================================================================
    // Terminator Emission
    // ========================================================================

    /// Emit a terminator.
    fn emit_terminator(&mut self, term: &Terminator<'db>, mir: &MirFunction<'db>) {
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
                self.emit_operand_pull(condition, mir);
                // JumpIfFalse to else_block
                let else_jump = self.emit(Instruction::JumpIfFalse(0));
                self.pending_jumps.push((else_jump, *else_block));
                // Jump to then_block (may be elided if it's next)
                self.emit_jump_unless_fallthrough(*then_block);
            }

            Terminator::Switch {
                discriminant,
                arms,
                otherwise,
            } => {
                self.emit_operand_pull(discriminant, mir);

                for (value, target) in arms {
                    self.emit(Instruction::Copy(0));
                    let idx = self.add_constant(Value::Int(*value));
                    self.emit(Instruction::LoadConst(idx));
                    self.emit(Instruction::CmpOp(CmpOp::Eq));
                    let jump_idx = self.emit(Instruction::JumpIfFalse(0));
                    self.emit(Instruction::Pop(1));
                    self.emit(Instruction::Pop(1));
                    self.emit_jump_unless_fallthrough(*target);
                    let skip_to = self.current_pc();
                    self.patch_jump_to(jump_idx, skip_to);
                }

                self.emit(Instruction::Pop(1));
                self.emit_jump_unless_fallthrough(*otherwise);
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
                let idx = self.add_constant(Value::Null);
                self.emit(Instruction::LoadConst(idx));
                self.emit(Instruction::Return);
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
    #[allow(clippy::cast_possible_wrap)]
    fn patch_jumps(&mut self) {
        for (instruction_idx, target_block) in self.pending_jumps.clone() {
            let target_pc = self.block_addresses[&target_block];
            let offset = target_pc as isize - instruction_idx as isize;

            match self.bytecode.instructions[instruction_idx] {
                Instruction::Jump(_) => {
                    self.bytecode.instructions[instruction_idx] = Instruction::Jump(offset);
                }
                Instruction::JumpIfFalse(_) => {
                    self.bytecode.instructions[instruction_idx] = Instruction::JumpIfFalse(offset);
                }
                _ => panic!("expected jump instruction at index {instruction_idx}"),
            }
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
            Instruction::JumpIfFalse(_) => {
                self.bytecode.instructions[instruction_idx] = Instruction::JumpIfFalse(offset);
            }
            _ => panic!("expected jump instruction at index {instruction_idx}"),
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
        mir: &MirFunction<'_>,
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
pub(crate) fn compile_mir_function(
    mir: &MirFunction<'_>,
    ctx: MirCodegenContext<'_, '_>,
) -> Function {
    // Run analysis
    let analysis = AnalysisResult::analyze(mir);

    // Compile with stackification
    let codegen = StackifyCodegen::new(ctx, analysis);
    codegen.compile(mir)
}
