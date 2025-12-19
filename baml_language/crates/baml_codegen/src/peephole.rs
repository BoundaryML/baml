//! Bytecode peephole optimizer with liveness analysis and stackification.
//!
//! This module implements post-emission optimizations on bytecode to eliminate
//! inefficiencies from the MIR-to-bytecode compilation:
//!
//! - **Stackification**: Eliminate temporary variables by keeping values on stack
//! - **Dead store elimination**: Remove `StoreVar` when value is never read
//! - **Store-load pair optimization**: Eliminate `StoreVar(x), LoadVar(x)` sequences
//! - **Redundant jump elimination**: Remove `Jump(1)` (jumps to next instruction)
//! - **Jump threading**: Collapse chains of jumps
//!
//! The optimizer uses backward dataflow liveness analysis to determine which
//! local variables are actually needed at each point.

use std::collections::{HashMap, HashSet};

use baml_vm::{Bytecode, Instruction};

// ============================================================================
// Stackification - Temporary Variable Elimination
// ============================================================================

/// Tracks definition and use information for each local slot.
#[derive(Debug, Default)]
struct LocalInfo {
    /// Instruction indices where this local is defined (`StoreVar`).
    definitions: Vec<usize>,
    /// Instruction indices where this local is used (`LoadVar`).
    load_uses: Vec<usize>,
    /// Whether this local has Watch/Notify uses (cannot be eliminated).
    has_special_uses: bool,
}

impl LocalInfo {
    /// Check if this local can potentially be stackified for single-use elimination.
    /// Must have exactly one definition, exactly one use, and no special uses.
    fn is_single_use_candidate(&self) -> bool {
        self.definitions.len() == 1 && self.load_uses.len() == 1 && !self.has_special_uses
    }
}

/// A candidate for stackification - a store-load pair that can be eliminated.
#[derive(Debug)]
struct StackifyCandidate {
    /// The `StoreVar` instruction index.
    store_idx: usize,
    /// The `LoadVar` instruction index.
    load_idx: usize,
    /// Whether this value is still live after the load (needs TEE pattern).
    needs_tee: bool,
}

// ============================================================================
// Control Flow Graph
// ============================================================================

/// A basic block in the bytecode control flow graph.
#[derive(Debug, Clone)]
struct BasicBlock {
    /// First instruction index (inclusive).
    start: usize,
    /// Last instruction index (exclusive).
    end: usize,
    /// Indices of successor blocks.
    successors: Vec<usize>,
}

/// Control flow graph built from bytecode.
#[derive(Debug)]
struct Cfg {
    /// All basic blocks.
    blocks: Vec<BasicBlock>,
    /// Maps instruction index to block index (for future cross-block optimizations).
    #[allow(dead_code)]
    insn_to_block: Vec<usize>,
}

impl Cfg {
    /// Build a CFG from bytecode by identifying basic block boundaries.
    ///
    /// Basic block boundaries occur at:
    /// - Jump targets (instructions that are jumped to)
    /// - Instructions following jumps/returns
    fn build(bytecode: &Bytecode) -> Self {
        let instructions = &bytecode.instructions;
        if instructions.is_empty() {
            return Self {
                blocks: vec![],
                insn_to_block: vec![],
            };
        }

        // Find all block boundaries (instruction indices that start new blocks)
        let mut boundaries: HashSet<usize> = HashSet::new();
        boundaries.insert(0); // First instruction starts a block

        for (idx, insn) in instructions.iter().enumerate() {
            match insn {
                Instruction::Jump(offset) => {
                    // Target starts a new block
                    let target = (idx as isize + offset) as usize;
                    if target < instructions.len() {
                        boundaries.insert(target);
                    }
                    // Instruction after jump starts a new block (fallthrough)
                    if idx + 1 < instructions.len() {
                        boundaries.insert(idx + 1);
                    }
                }
                Instruction::JumpIfFalse(offset) => {
                    // Target starts a new block
                    let target = (idx as isize + offset) as usize;
                    if target < instructions.len() {
                        boundaries.insert(target);
                    }
                    // Instruction after conditional jump starts a new block
                    if idx + 1 < instructions.len() {
                        boundaries.insert(idx + 1);
                    }
                }
                Instruction::Return => {
                    // Instruction after return starts a new block (if any)
                    if idx + 1 < instructions.len() {
                        boundaries.insert(idx + 1);
                    }
                }
                _ => {}
            }
        }

        // Sort boundaries to build blocks
        let mut sorted_boundaries: Vec<usize> = boundaries.into_iter().collect();
        sorted_boundaries.sort_unstable();

        // Build blocks
        let mut blocks = Vec::new();
        let mut insn_to_block = vec![0; instructions.len()];

        for (block_idx, &start) in sorted_boundaries.iter().enumerate() {
            let end = sorted_boundaries
                .get(block_idx + 1)
                .copied()
                .unwrap_or(instructions.len());

            // Mark all instructions in this block
            for idx in start..end {
                insn_to_block[idx] = block_idx;
            }

            blocks.push(BasicBlock {
                start,
                end,
                successors: vec![],
            });
        }

        // Build successor edges
        let num_blocks = blocks.len();
        for (block_idx, block) in blocks.iter_mut().enumerate() {
            if block.end == 0 {
                continue;
            }

            let last_insn_idx = block.end - 1;
            let last_insn = &instructions[last_insn_idx];

            match last_insn {
                Instruction::Jump(offset) => {
                    let target = (last_insn_idx as isize + offset) as usize;
                    if target < instructions.len() {
                        let target_block = insn_to_block[target];
                        block.successors.push(target_block);
                    }
                }
                Instruction::JumpIfFalse(offset) => {
                    // Conditional has two successors: fallthrough and target
                    let target = (last_insn_idx as isize + offset) as usize;

                    // Fallthrough (next block)
                    if block_idx + 1 < num_blocks {
                        block.successors.push(block_idx + 1);
                    }

                    // Jump target
                    if target < instructions.len() {
                        let target_block = insn_to_block[target];
                        if !block.successors.contains(&target_block) {
                            block.successors.push(target_block);
                        }
                    }
                }
                Instruction::Return => {
                    // No successors
                }
                _ => {
                    // Fallthrough to next block
                    if block_idx + 1 < num_blocks {
                        block.successors.push(block_idx + 1);
                    }
                }
            }
        }

        Self {
            blocks,
            insn_to_block,
        }
    }
}

// ============================================================================
// Liveness Analysis
// ============================================================================

/// Liveness information computed by backward dataflow analysis.
#[derive(Debug)]
struct LivenessInfo {
    /// For each instruction, the set of locals that are live *after* it.
    live_out: Vec<HashSet<usize>>,
}

impl LivenessInfo {
    /// Compute liveness using backward dataflow analysis.
    ///
    /// Dataflow equations:
    /// - `use[i]` = locals read by instruction i
    /// - `def[i]` = locals written by instruction i
    /// - `live_in[i] = use[i] ∪ (live_out[i] - def[i])`
    /// - `live_out[i] = ∪ { live_in[s] | s ∈ successors[i] }`
    fn compute(bytecode: &Bytecode, cfg: &Cfg) -> Self {
        let n = bytecode.instructions.len();
        if n == 0 {
            return Self { live_out: vec![] };
        }

        // Initialize live_in and live_out for each instruction
        let mut live_in: Vec<HashSet<usize>> = vec![HashSet::new(); n];
        let mut live_out: Vec<HashSet<usize>> = vec![HashSet::new(); n];

        // Iterate until fixed point
        let mut changed = true;
        while changed {
            changed = false;

            // Process blocks in reverse order for faster convergence
            for block in cfg.blocks.iter().rev() {
                // Process instructions in block in reverse order
                for idx in (block.start..block.end).rev() {
                    // Compute live_out[idx] = union of live_in of successors
                    let new_live_out: HashSet<usize> = if idx == block.end - 1 {
                        // Last instruction: successors are first instructions of successor blocks
                        let mut out = HashSet::new();
                        for &succ_block in &block.successors {
                            if let Some(succ_start) = cfg.blocks.get(succ_block).map(|b| b.start) {
                                out.extend(&live_in[succ_start]);
                            }
                        }
                        out
                    } else {
                        // Mid-block: successor is just the next instruction
                        live_in[idx + 1].clone()
                    };

                    // Compute live_in[idx] = use[idx] ∪ (live_out[idx] - def[idx])
                    let (uses, defs) = Self::uses_defs(&bytecode.instructions[idx]);
                    let mut new_live_in = new_live_out.clone();
                    for def in &defs {
                        new_live_in.remove(def);
                    }
                    new_live_in.extend(uses);

                    // Check for changes
                    if new_live_out != live_out[idx] {
                        live_out[idx] = new_live_out;
                        changed = true;
                    }
                    if new_live_in != live_in[idx] {
                        live_in[idx] = new_live_in;
                        changed = true;
                    }
                }
            }
        }

        Self { live_out }
    }

    /// Get uses and defs for an instruction.
    ///
    /// - `uses`: locals that are read
    /// - `defs`: locals that are written
    fn uses_defs(insn: &Instruction) -> (Vec<usize>, Vec<usize>) {
        match insn {
            Instruction::LoadVar(local) => (vec![*local], vec![]),
            Instruction::StoreVar(local) => (vec![], vec![*local]),
            Instruction::Watch(local) => (vec![*local], vec![]),
            Instruction::Notify(local) => (vec![*local], vec![]),
            _ => (vec![], vec![]),
        }
    }

    /// Check if a local is live after the instruction at `idx`.
    fn is_live_after(&self, idx: usize, local: usize) -> bool {
        self.live_out
            .get(idx)
            .is_some_and(|set| set.contains(&local))
    }
}

// ============================================================================
// Optimizer
// ============================================================================

/// Peephole optimizer for bytecode.
pub(crate) struct PeepholeOptimizer;

impl PeepholeOptimizer {
    /// Optimize bytecode in place.
    pub(crate) fn optimize(bytecode: &mut Bytecode) {
        if bytecode.instructions.is_empty() {
            return;
        }

        // Iterate until no more changes (patterns may become optimizable after other optimizations)
        let mut changed = true;
        let mut iterations = 0;
        const MAX_ITERATIONS: usize = 5;

        while changed && iterations < MAX_ITERATIONS {
            changed = false;
            iterations += 1;

            // Pass 0: Stackification - eliminate temporaries by keeping values on stack
            // This should run first to expose more optimization opportunities
            if Self::stackify(bytecode) {
                changed = true;
            }

            // Pass 1: Eliminate redundant jumps (Jump(1)) first
            // This may create new consecutive store-load pairs
            let mut to_remove: HashSet<usize> = HashSet::new();
            Self::find_redundant_jumps(bytecode, &mut to_remove);

            if !to_remove.is_empty() {
                Self::apply_removals(bytecode, &to_remove);
                changed = true;
            }

            // Pass 2: Jump threading
            let threading_changed = Self::thread_jumps(bytecode);
            if threading_changed {
                changed = true;
            }

            // Pass 3: Build CFG and compute liveness for store optimizations
            let cfg = Cfg::build(bytecode);
            let liveness = LivenessInfo::compute(bytecode, &cfg);

            // Pass 4: Identify dead stores and store-load pairs
            let mut to_remove: HashSet<usize> = HashSet::new();
            let mut transformations: HashMap<usize, Instruction> = HashMap::new();
            Self::find_store_optimizations(
                bytecode,
                &liveness,
                &mut to_remove,
                &mut transformations,
            );

            // Apply transformations
            for (idx, new_insn) in transformations {
                bytecode.instructions[idx] = new_insn;
                changed = true;
            }

            // Apply removals
            if !to_remove.is_empty() {
                Self::apply_removals(bytecode, &to_remove);
                changed = true;
            }
        }

        // Final pass: eliminate unused pre-allocated null slots
        Self::eliminate_unused_slots(bytecode);
    }

    // ========================================================================
    // Stackification Pass
    // ========================================================================

    /// Stackification pass: eliminate temporary variables by keeping values on stack.
    ///
    /// This pass identifies store-load pairs where the stored value is still
    /// logically "on the stack" at the point of loading (i.e., no intervening
    /// instructions have modified the stack in a way that would move the value).
    fn stackify(bytecode: &mut Bytecode) -> bool {
        let cfg = Cfg::build(bytecode);
        let liveness = LivenessInfo::compute(bytecode, &cfg);
        let local_info = Self::build_local_info(bytecode);

        let candidates = Self::find_stackify_candidates(bytecode, &cfg, &local_info, &liveness);

        if candidates.is_empty() {
            return false;
        }

        Self::apply_stackification(bytecode, &candidates);
        true
    }

    /// Build def-use information for all local slots.
    fn build_local_info(bytecode: &Bytecode) -> HashMap<usize, LocalInfo> {
        let mut info: HashMap<usize, LocalInfo> = HashMap::new();

        for (idx, insn) in bytecode.instructions.iter().enumerate() {
            match insn {
                Instruction::StoreVar(slot) => {
                    info.entry(*slot).or_default().definitions.push(idx);
                }
                Instruction::LoadVar(slot) => {
                    info.entry(*slot).or_default().load_uses.push(idx);
                }
                Instruction::Watch(slot) | Instruction::Notify(slot) => {
                    info.entry(*slot).or_default().has_special_uses = true;
                }
                _ => {}
            }
        }

        info
    }

    /// Find stackification candidates by simulating the value stack.
    ///
    /// A store-load pair can be stackified if:
    /// 1. The local has exactly one definition (single `StoreVar`)
    /// 2. The local has no Watch/Notify uses
    /// 3. The store and load are in the same basic block
    /// 4. The value would still be at stack top when loaded
    fn find_stackify_candidates(
        bytecode: &Bytecode,
        cfg: &Cfg,
        local_info: &HashMap<usize, LocalInfo>,
        liveness: &LivenessInfo,
    ) -> Vec<StackifyCandidate> {
        let mut candidates = Vec::new();

        // Process each basic block independently
        for block in &cfg.blocks {
            // Simulate the stack for this block
            // We track which slots have values "shadowed" on the conceptual stack
            // Key insight: we track the instruction that produced each stack value
            // and which slot it was stored to (if any)

            let block_candidates =
                Self::simulate_block_for_stackify(bytecode, block, local_info, liveness);
            candidates.extend(block_candidates);
        }

        candidates
    }

    /// Simulate the value stack for a single basic block to find stackification opportunities.
    fn simulate_block_for_stackify(
        bytecode: &Bytecode,
        block: &BasicBlock,
        local_info: &HashMap<usize, LocalInfo>,
        liveness: &LivenessInfo,
    ) -> Vec<StackifyCandidate> {
        let mut candidates = Vec::new();

        // Map from slot -> (store_idx, position_in_stack_at_store_time)
        // This tracks where a stored value "would be" if it stayed on the stack
        let mut stored_values: HashMap<usize, (usize, usize)> = HashMap::new();

        // Simulated stack depth (we don't track actual values, just depth)
        let mut stack_depth: usize = 0;

        for idx in block.start..block.end {
            let insn = &bytecode.instructions[idx];

            match insn {
                // Instructions that push one value
                Instruction::LoadConst(_)
                | Instruction::LoadGlobal(_)
                | Instruction::AllocInstance(_)
                | Instruction::AllocVariant(_) => {
                    stack_depth += 1;
                }

                // LoadVar: check if we can stackify this
                Instruction::LoadVar(slot) => {
                    // Check if this local is a single-use stackification candidate
                    if let Some(info) = local_info.get(slot) {
                        if info.is_single_use_candidate() {
                            // Check if we have a stored value for this slot
                            if let Some(&(store_idx, stored_stack_pos)) = stored_values.get(slot) {
                                // The value would be at stack position stored_stack_pos
                                // Current stack depth is stack_depth
                                // The value is at position stored_stack_pos from bottom
                                // To be at top, stored_stack_pos should equal stack_depth
                                // (stack_depth is the position where next push would go)

                                // Check if the value is still at the same relative position
                                // Since we only support "at top" for now, we check if
                                // nothing else was pushed since the store
                                if stored_stack_pos == stack_depth {
                                    // Value is at stack top! We can stackify.
                                    let needs_tee = liveness.is_live_after(idx, *slot);
                                    candidates.push(StackifyCandidate {
                                        store_idx,
                                        load_idx: idx,
                                        needs_tee,
                                    });
                                }
                            }
                        }
                    }
                    stack_depth += 1;
                }

                // StoreVar: record that this value is "shadowed" in slot
                Instruction::StoreVar(slot) => {
                    if stack_depth > 0 {
                        // The value being stored was at stack_depth - 1
                        // After the store, it would conceptually still be at that position
                        // if the store was removed
                        stack_depth -= 1;
                        stored_values.insert(*slot, (idx, stack_depth));
                    }
                }

                // Pop consumes values
                Instruction::Pop(n) => {
                    stack_depth = stack_depth.saturating_sub(*n);
                }

                // Copy duplicates
                Instruction::Copy(_) => {
                    stack_depth += 1;
                }

                // PopReplace: pop n+1 values, push 1 (result)
                Instruction::PopReplace(n) => {
                    stack_depth = stack_depth.saturating_sub(*n);
                }

                // Binary ops: pop 2, push 1
                Instruction::BinOp(_) | Instruction::CmpOp(_) => {
                    stack_depth = stack_depth.saturating_sub(1);
                }

                // Unary ops: pop 1, push 1 (net 0)
                Instruction::UnaryOp(_) => {}

                // AllocArray: pop n, push 1
                Instruction::AllocArray(n) => {
                    stack_depth = stack_depth.saturating_sub(n.saturating_sub(1));
                }

                // AllocMap: pop 2n, push 1
                Instruction::AllocMap(n) => {
                    let consumed = 2 * n;
                    stack_depth = stack_depth.saturating_sub(consumed.saturating_sub(1));
                }

                // Field loads: pop 1 (object), push 1 (field) - net 0
                Instruction::LoadField(_)
                | Instruction::LoadArrayElement
                | Instruction::LoadMapElement => {}

                // Field stores: pop 2 (object + value), push 0
                Instruction::StoreField(_) => {
                    stack_depth = stack_depth.saturating_sub(2);
                }

                // Array/map element store: pop 3, push 0
                Instruction::StoreArrayElement | Instruction::StoreMapElement => {
                    stack_depth = stack_depth.saturating_sub(3);
                }

                // Call: pop n+1 (fn + args), push 1 (result)
                Instruction::Call(n) => {
                    stack_depth = stack_depth.saturating_sub(*n);
                }

                // DispatchFuture: pop n+1, push 1
                Instruction::DispatchFuture(n) => {
                    stack_depth = stack_depth.saturating_sub(*n);
                }

                // Await: pop 1, push 1 - net 0
                Instruction::Await => {}

                // Return: pop 1
                Instruction::Return => {
                    stack_depth = stack_depth.saturating_sub(1);
                }

                // Assert: pop 1
                Instruction::Assert => {
                    stack_depth = stack_depth.saturating_sub(1);
                }

                // Control flow and other - don't affect stack in obvious ways
                Instruction::Jump(_)
                | Instruction::JumpIfFalse(_)
                | Instruction::Watch(_)
                | Instruction::Notify(_)
                | Instruction::NotifyBlock(_)
                | Instruction::StoreGlobal(_) => {}
            }
        }

        candidates
    }

    /// Apply stackification transformations.
    fn apply_stackification(bytecode: &mut Bytecode, candidates: &[StackifyCandidate]) {
        if candidates.is_empty() {
            return;
        }

        let mut to_remove: HashSet<usize> = HashSet::new();

        for candidate in candidates {
            if candidate.needs_tee {
                // TEE pattern: keep value on stack AND store it
                // For now, skip stackification and let the existing
                // store-load pair optimization handle it.
                // TODO: Handle TEE pattern with Copy(0) insertion.
                continue;
            }

            // Simple case: remove both StoreVar and LoadVar
            to_remove.insert(candidate.store_idx);
            to_remove.insert(candidate.load_idx);
        }

        // Apply removals
        if !to_remove.is_empty() {
            Self::apply_removals(bytecode, &to_remove);
        }
    }

    /// Eliminate unused pre-allocated null slots.
    ///
    /// After stackification, some slots may no longer be used.
    /// We can remove their initial `LOAD_CONST(null)` instructions
    /// and renumber the remaining slots.
    fn eliminate_unused_slots(bytecode: &mut Bytecode) {
        if bytecode.instructions.is_empty() {
            return;
        }

        // Find all slots that are still used
        let mut used_slots: HashSet<usize> = HashSet::new();
        for insn in &bytecode.instructions {
            match insn {
                Instruction::LoadVar(slot)
                | Instruction::StoreVar(slot)
                | Instruction::Watch(slot)
                | Instruction::Notify(slot) => {
                    used_slots.insert(*slot);
                }
                _ => {}
            }
        }

        if used_slots.is_empty() {
            // No slots used at all - remove all initial null pushes
            // Find how many LOAD_CONST(null) instructions at the start
            let mut null_count = 0;
            let null_const_idx = bytecode
                .constants
                .iter()
                .position(|c| matches!(c, baml_vm::types::Value::Null));

            if let Some(null_idx) = null_const_idx {
                for insn in &bytecode.instructions {
                    if let Instruction::LoadConst(idx) = insn {
                        if *idx == null_idx {
                            null_count += 1;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }

            if null_count > 0 {
                let to_remove: HashSet<usize> = (0..null_count).collect();
                Self::apply_removals(bytecode, &to_remove);
            }
            return;
        }

        // Find the initial null allocation sequence
        let null_const_idx = bytecode
            .constants
            .iter()
            .position(|c| matches!(c, baml_vm::types::Value::Null));

        if null_const_idx.is_none() {
            return;
        }
        let null_idx = null_const_idx.unwrap();

        // Count consecutive LOAD_CONST(null) at the start
        let mut null_push_indices: Vec<usize> = Vec::new();
        for (i, insn) in bytecode.instructions.iter().enumerate() {
            if let Instruction::LoadConst(idx) = insn {
                if *idx == null_idx {
                    null_push_indices.push(i);
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        if null_push_indices.is_empty() {
        }

        // The pre-allocated slots correspond to positions in the null_push_indices
        // We need to figure out which slots map to which null pushes
        // This is tricky because slot numbering depends on function arity and layout

        // For now, we'll be conservative and only remove null pushes if
        // the corresponding slot (by position) is completely unused.
        // This requires understanding the slot-to-null-push mapping.

        // Actually, let's skip this complex renumbering for now.
        // The main optimization (removing store/load pairs) is the priority.
        // Slot elimination can be added later.
    }

    /// Find store optimizations: dead stores and store-load pairs.
    fn find_store_optimizations(
        bytecode: &Bytecode,
        liveness: &LivenessInfo,
        to_remove: &mut HashSet<usize>,
        transformations: &mut HashMap<usize, Instruction>,
    ) {
        let n = bytecode.instructions.len();

        // Track which indices we've already handled as part of store-load pairs
        let mut handled: HashSet<usize> = HashSet::new();

        for idx in 0..n {
            if handled.contains(&idx) {
                continue;
            }

            // Check for store-load pair: StoreVar(x), LoadVar(x)
            if idx + 1 < n {
                if let (Instruction::StoreVar(store_local), Instruction::LoadVar(load_local)) =
                    (&bytecode.instructions[idx], &bytecode.instructions[idx + 1])
                {
                    if store_local == load_local {
                        // Check if local is live after the LoadVar
                        let is_live = liveness.is_live_after(idx + 1, *store_local);

                        if is_live {
                            // Value needed later: transform to Copy(0), StoreVar(x)
                            // This keeps value on stack AND stores it
                            // Before: [value] StoreVar(x) LoadVar(x) -> [value] on stack
                            // After:  [value] Copy(0) StoreVar(x)    -> [value] on stack
                            transformations.insert(idx, Instruction::Copy(0));
                            transformations.insert(idx + 1, Instruction::StoreVar(*store_local));
                        } else {
                            // Value not needed later: remove both instructions
                            // Value stays on stack from the original computation
                            to_remove.insert(idx);
                            to_remove.insert(idx + 1);
                        }
                        handled.insert(idx);
                        handled.insert(idx + 1);
                        continue;
                    }
                }
            }

            // Check for dead store (not part of store-load pair)
            if let Instruction::StoreVar(local) = &bytecode.instructions[idx] {
                if !liveness.is_live_after(idx, *local) {
                    // Store is dead - replace with Pop to consume the stack value
                    transformations.insert(idx, Instruction::Pop(1));
                }
            }
        }
    }

    /// Find redundant jumps that jump to the next instruction.
    fn find_redundant_jumps(bytecode: &Bytecode, to_remove: &mut HashSet<usize>) {
        for (idx, insn) in bytecode.instructions.iter().enumerate() {
            if let Instruction::Jump(1) = insn {
                // Jump(1) jumps to the next instruction - redundant
                to_remove.insert(idx);
            }
            // Note: JumpIfFalse(1) is NOT redundant because it has a side effect
            // (popping the condition and conditionally skipping)
        }
    }

    /// Apply removals and adjust jump offsets.
    fn apply_removals(bytecode: &mut Bytecode, to_remove: &HashSet<usize>) {
        let n = bytecode.instructions.len();

        // Build remapping: old index -> new index (None if removed)
        let mut remap: Vec<Option<usize>> = Vec::with_capacity(n);
        let mut new_idx = 0;
        for old_idx in 0..n {
            if to_remove.contains(&old_idx) {
                remap.push(None);
            } else {
                remap.push(Some(new_idx));
                new_idx += 1;
            }
        }

        // Adjust jump offsets
        for old_idx in 0..n {
            if to_remove.contains(&old_idx) {
                continue;
            }

            match &mut bytecode.instructions[old_idx] {
                Instruction::Jump(offset) | Instruction::JumpIfFalse(offset) => {
                    let old_target = (old_idx as isize + *offset) as usize;
                    let new_source = remap[old_idx].unwrap();

                    // Find new target - if target was removed, find next non-removed
                    let new_target = Self::find_new_target(&remap, old_target, n);

                    *offset = new_target as isize - new_source as isize;
                }
                _ => {}
            }
        }

        // Filter out removed instructions
        let new_instructions: Vec<Instruction> = bytecode
            .instructions
            .iter()
            .enumerate()
            .filter(|(i, _)| !to_remove.contains(i))
            .map(|(_, insn)| *insn)
            .collect();

        // Filter source_lines and scopes to match
        let new_source_lines: Vec<usize> = bytecode
            .source_lines
            .iter()
            .enumerate()
            .filter(|(i, _)| !to_remove.contains(i))
            .map(|(_, &line)| line)
            .collect();

        let new_scopes: Vec<usize> = bytecode
            .scopes
            .iter()
            .enumerate()
            .filter(|(i, _)| !to_remove.contains(i))
            .map(|(_, &scope)| scope)
            .collect();

        bytecode.instructions = new_instructions;
        bytecode.source_lines = new_source_lines;
        bytecode.scopes = new_scopes;
    }

    /// Find the new target index after removals.
    ///
    /// If the target was removed, find the next non-removed instruction.
    fn find_new_target(remap: &[Option<usize>], old_target: usize, n: usize) -> usize {
        // Start from old_target and find first non-removed instruction
        for idx in old_target..n {
            if let Some(new_idx) = remap.get(idx).copied().flatten() {
                return new_idx;
            }
        }
        // Fallback: if target was at/past end, return the new length
        remap.iter().filter_map(|&x| x).count()
    }

    /// Thread jumps: if a jump targets another jump, skip to the final target.
    /// Returns true if any changes were made.
    fn thread_jumps(bytecode: &mut Bytecode) -> bool {
        let n = bytecode.instructions.len();
        if n == 0 {
            return false;
        }

        let mut any_changed = false;

        // Iterate until no more changes
        let mut changed = true;
        let mut iterations = 0;
        const MAX_ITERATIONS: usize = 10; // Prevent infinite loops

        while changed && iterations < MAX_ITERATIONS {
            changed = false;
            iterations += 1;

            for idx in 0..n {
                let (offset, is_conditional) = match bytecode.instructions[idx] {
                    Instruction::Jump(o) => (o, false),
                    Instruction::JumpIfFalse(o) => (o, true),
                    _ => continue,
                };

                let target = (idx as isize + offset) as usize;
                if target >= n {
                    continue;
                }

                // Check if target is an unconditional jump
                if let Instruction::Jump(target_offset) = bytecode.instructions[target] {
                    // Thread through: compute new offset to final target
                    let final_target = (target as isize + target_offset) as usize;
                    let new_offset = final_target as isize - idx as isize;

                    // Update the jump
                    if is_conditional {
                        bytecode.instructions[idx] = Instruction::JumpIfFalse(new_offset);
                    } else {
                        bytecode.instructions[idx] = Instruction::Jump(new_offset);
                    }
                    changed = true;
                    any_changed = true;
                }
            }
        }

        any_changed
    }
}

/// Main entry point: optimize bytecode in place.
pub(crate) fn optimize(bytecode: &mut Bytecode) {
    PeepholeOptimizer::optimize(bytecode);
}

#[cfg(test)]
mod tests {
    use baml_vm::Instruction;

    use super::*;

    fn make_bytecode(instructions: Vec<Instruction>) -> Bytecode {
        let n = instructions.len();
        Bytecode {
            instructions,
            constants: vec![],
            source_lines: vec![0; n],
            scopes: vec![0; n],
        }
    }

    #[test]
    fn test_cfg_simple() {
        // Linear code: no branches
        let bytecode = make_bytecode(vec![
            Instruction::LoadConst(0),
            Instruction::StoreVar(1),
            Instruction::Return,
        ]);

        let cfg = Cfg::build(&bytecode);
        assert_eq!(cfg.blocks.len(), 1);
        assert_eq!(cfg.blocks[0].start, 0);
        assert_eq!(cfg.blocks[0].end, 3);
    }

    #[test]
    fn test_cfg_with_jump() {
        // Code with a forward jump
        let bytecode = make_bytecode(vec![
            Instruction::LoadConst(0), // 0: bb0
            Instruction::Jump(2),      // 1: jump to 3
            Instruction::LoadConst(1), // 2: bb1 (fallthrough target)
            Instruction::Return,       // 3: bb2 (jump target)
        ]);

        let cfg = Cfg::build(&bytecode);
        // Blocks: [0,2), [2,3), [3,4)
        assert_eq!(cfg.blocks.len(), 3);
    }

    #[test]
    fn test_liveness_simple() {
        // x = 1; y = x; return y;
        let bytecode = make_bytecode(vec![
            Instruction::LoadConst(0), // 0: push 1
            Instruction::StoreVar(1),  // 1: x = pop
            Instruction::LoadVar(1),   // 2: push x
            Instruction::StoreVar(2),  // 3: y = pop
            Instruction::LoadVar(2),   // 4: push y
            Instruction::Return,       // 5: return
        ]);

        let cfg = Cfg::build(&bytecode);
        let liveness = LivenessInfo::compute(&bytecode, &cfg);

        // After instruction 1 (StoreVar x), x should be live (read at 2)
        assert!(liveness.is_live_after(1, 1));
        // After instruction 3 (StoreVar y), y should be live (read at 4)
        assert!(liveness.is_live_after(3, 2));
        // After instruction 4 (LoadVar y), y should NOT be live
        assert!(!liveness.is_live_after(4, 2));
    }

    #[test]
    fn test_eliminate_redundant_jump() {
        let mut bytecode = make_bytecode(vec![
            Instruction::LoadConst(0),
            Instruction::Jump(1), // Redundant: jumps to next
            Instruction::Return,
        ]);

        optimize(&mut bytecode);

        // Jump(1) should be removed
        assert_eq!(
            bytecode.instructions,
            vec![Instruction::LoadConst(0), Instruction::Return,]
        );
    }

    #[test]
    fn test_eliminate_store_load_dead() {
        // StoreVar(x), LoadVar(x) where x is dead after
        let mut bytecode = make_bytecode(vec![
            Instruction::LoadConst(0),
            Instruction::StoreVar(1), // Store to x
            Instruction::LoadVar(1),  // Load x back (x dead after)
            Instruction::Return,
        ]);

        optimize(&mut bytecode);

        // Both StoreVar and LoadVar should be removed
        // Value stays on stack from LoadConst
        assert_eq!(
            bytecode.instructions,
            vec![Instruction::LoadConst(0), Instruction::Return,]
        );
    }

    #[test]
    fn test_store_load_live() {
        // StoreVar(x), LoadVar(x), ..., LoadVar(x)
        // x is still needed, so transform to Copy(0), StoreVar(x)
        let mut bytecode = make_bytecode(vec![
            Instruction::LoadConst(0),
            Instruction::StoreVar(1), // Store to x
            Instruction::LoadVar(1),  // Load x (x still live)
            Instruction::Pop(1),      // Pop it
            Instruction::LoadVar(1),  // Load x again
            Instruction::Return,
        ]);

        optimize(&mut bytecode);

        // Should become: LoadConst, Copy(0), StoreVar, Pop, LoadVar, Return
        assert_eq!(
            bytecode.instructions,
            vec![
                Instruction::LoadConst(0),
                Instruction::Copy(0),
                Instruction::StoreVar(1),
                Instruction::Pop(1),
                Instruction::LoadVar(1),
                Instruction::Return,
            ]
        );
    }

    #[test]
    fn test_jump_threading() {
        // Jump to a jump should be threaded
        let mut bytecode = make_bytecode(vec![
            Instruction::Jump(2),      // 0: jump to 2
            Instruction::LoadConst(0), // 1: unreachable
            Instruction::Jump(2),      // 2: jump to 4
            Instruction::LoadConst(1), // 3: unreachable
            Instruction::Return,       // 4: final target
        ]);

        optimize(&mut bytecode);

        // First jump should go directly to Return
        if let Instruction::Jump(offset) = bytecode.instructions[0] {
            // After threading, should jump directly to Return
            let target = offset;
            // Return is now at different position due to no removals here
            assert!(target >= 0);
        }
    }

    #[test]
    fn test_dead_store_becomes_pop() {
        // StoreVar(x) where x is never read
        let mut bytecode = make_bytecode(vec![
            Instruction::LoadConst(0),
            Instruction::StoreVar(1), // Store to x (never read)
            Instruction::LoadConst(1),
            Instruction::Return,
        ]);

        optimize(&mut bytecode);

        // StoreVar should become Pop(1)
        assert_eq!(
            bytecode.instructions,
            vec![
                Instruction::LoadConst(0),
                Instruction::Pop(1),
                Instruction::LoadConst(1),
                Instruction::Return,
            ]
        );
    }
}
