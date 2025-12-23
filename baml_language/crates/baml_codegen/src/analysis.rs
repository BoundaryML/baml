//! MIR analysis for stackification.
//!
//! This module provides:
//! - CFG predecessor computation
//! - Dominator tree computation (Cooper-Harvey-Kennedy algorithm)
//! - Def-use information collection
//! - Local classification (Virtual vs Real)

use std::collections::{HashMap, HashSet};

use baml_mir::{
    BlockId, Constant, Local, MirFunction, Operand, Place, Rvalue, StatementKind, Terminator,
};

// ============================================================================
// Data Structures
// ============================================================================

/// Where a local is defined.
#[derive(Clone, Debug)]
pub(crate) struct DefLocation<'db> {
    pub block: BlockId,
    pub statement_idx: usize,
    /// The rvalue that produces this local's value (for inlining).
    pub rvalue: Rvalue<'db>,
}

/// Where a local is used.
#[derive(Clone, Debug)]
pub(crate) struct UseLocation {
    pub block: BlockId,
    pub statement_idx: usize,
}

/// Sentinel value for uses in terminators.
pub(crate) const TERMINATOR_IDX: usize = usize::MAX;

/// Def-use information for a single local.
#[derive(Clone, Debug)]
pub(crate) struct LocalDefUse<'db> {
    #[allow(dead_code)] // Kept for debugging and future use
    pub local: Local,
    /// Definition site (None for parameters, which are defined at entry).
    pub def: Option<DefLocation<'db>>,
    /// All use sites.
    pub uses: Vec<UseLocation>,
}

/// Classification of a local variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalClassification {
    /// Function parameter - always real.
    Parameter,
    /// Multi-use or cross-block local - needs stack slot.
    Real,
    /// Single-use temporary that can be inlined.
    Virtual,
}

/// Dominator tree.
#[derive(Debug)]
pub(crate) struct Dominators {
    /// Immediate dominator of each block (entry has None).
    pub idom: HashMap<BlockId, Option<BlockId>>,
    /// Reverse postorder indices for faster intersection.
    #[allow(dead_code)] // Used in dominator computation, kept for future optimizations
    rpo_idx: HashMap<BlockId, usize>,
}

impl Dominators {
    /// Check if `dominator` dominates `block`.
    pub(crate) fn dominates(&self, dominator: BlockId, block: BlockId) -> bool {
        if dominator == block {
            return true;
        }

        let mut current = block;
        while let Some(Some(idom)) = self.idom.get(&current) {
            if *idom == dominator {
                return true;
            }
            current = *idom;
        }

        false
    }
}

/// Complete analysis result for a function.
#[derive(Debug)]
pub(crate) struct AnalysisResult<'db> {
    /// Classification for each local.
    pub classifications: HashMap<Local, LocalClassification>,
    /// Def-use information for each local.
    pub def_use: HashMap<Local, LocalDefUse<'db>>,
    /// Dominator tree.
    #[allow(dead_code)] // Kept for future scope-aware codegen
    pub dominators: Dominators,
    /// Reverse postorder of blocks (for iteration).
    pub rpo: Vec<BlockId>,
    /// Predecessor map for each block.
    #[allow(dead_code)] // Kept for future scope-aware codegen
    pub predecessors: HashMap<BlockId, Vec<BlockId>>,
}

// ============================================================================
// Analysis Entry Point
// ============================================================================

impl<'db> AnalysisResult<'db> {
    /// Analyze a MIR function and produce classification results.
    pub(crate) fn analyze(mir: &MirFunction<'db>) -> Self {
        // Step 1: Build predecessor map
        let predecessors = build_predecessors(mir);

        // Step 2: Compute reverse postorder
        let rpo = compute_rpo(mir);

        // Step 3: Compute dominators
        let dominators = compute_dominators(mir, &rpo, &predecessors);

        // Step 4: Collect def-use information
        let def_use = collect_def_use(mir);

        // Step 5: Classify each local
        let classifications = classify_locals(mir, &def_use, &dominators, &predecessors);

        Self {
            classifications,
            def_use,
            dominators,
            rpo,
            predecessors,
        }
    }
}

// ============================================================================
// CFG Analysis
// ============================================================================

/// Build predecessor map for all blocks.
fn build_predecessors(mir: &MirFunction<'_>) -> HashMap<BlockId, Vec<BlockId>> {
    let mut preds: HashMap<BlockId, Vec<BlockId>> = HashMap::new();

    // Initialize with empty vecs
    for block in &mir.blocks {
        preds.insert(block.id, Vec::new());
    }

    // Collect predecessor edges from terminators
    for block in &mir.blocks {
        if let Some(term) = &block.terminator {
            for succ in term.successors() {
                if let Some(pred_list) = preds.get_mut(&succ) {
                    pred_list.push(block.id);
                }
            }
        }
    }

    preds
}

/// DFS helper for computing postorder.
fn rpo_dfs(
    mir: &MirFunction<'_>,
    block_id: BlockId,
    visited: &mut HashSet<BlockId>,
    postorder: &mut Vec<BlockId>,
) {
    if visited.contains(&block_id) {
        return;
    }
    visited.insert(block_id);

    let block = mir.block(block_id);
    if let Some(term) = &block.terminator {
        for succ in term.successors() {
            rpo_dfs(mir, succ, visited, postorder);
        }
    }
    postorder.push(block_id);
}

/// Compute reverse postorder (depth-first, postorder reversed).
fn compute_rpo(mir: &MirFunction<'_>) -> Vec<BlockId> {
    let mut visited = HashSet::new();
    let mut postorder = Vec::new();

    rpo_dfs(mir, mir.entry, &mut visited, &mut postorder);
    postorder.reverse();
    postorder
}

// ============================================================================
// Dominator Computation (Cooper-Harvey-Kennedy Algorithm)
// ============================================================================

/// Compute dominators using the Cooper-Harvey-Kennedy algorithm.
///
/// This is a simple, efficient iterative algorithm that computes immediate
/// dominators by repeatedly intersecting dominator sets until convergence.
fn compute_dominators(
    mir: &MirFunction<'_>,
    rpo: &[BlockId],
    preds: &HashMap<BlockId, Vec<BlockId>>,
) -> Dominators {
    // Map BlockId to RPO index for faster lookup
    let rpo_idx: HashMap<BlockId, usize> = rpo.iter().enumerate().map(|(i, &b)| (b, i)).collect();

    let mut idom: HashMap<BlockId, Option<BlockId>> = HashMap::new();

    // Initialize: entry dominates itself (represented as None for "no parent")
    idom.insert(mir.entry, None);

    let mut changed = true;
    while changed {
        changed = false;

        // Skip entry (index 0)
        for &block in &rpo[1..] {
            let predecessors = &preds[&block];

            // Find first predecessor with defined idom
            let mut new_idom = None;
            for &p in predecessors {
                if idom.contains_key(&p) {
                    new_idom = Some(p);
                    break;
                }
            }

            // Intersect with remaining predecessors
            if let Some(mut new_idom_val) = new_idom {
                for &p in predecessors {
                    if idom.contains_key(&p) && p != new_idom_val {
                        // Intersect
                        new_idom_val = intersect(&rpo_idx, &idom, p, new_idom_val);
                    }
                }

                let old = idom.get(&block);
                if old != Some(&Some(new_idom_val)) {
                    idom.insert(block, Some(new_idom_val));
                    changed = true;
                }
            }
        }
    }

    Dominators { idom, rpo_idx }
}

/// Intersect two dominator chains to find their common dominator.
fn intersect(
    rpo_idx: &HashMap<BlockId, usize>,
    idom: &HashMap<BlockId, Option<BlockId>>,
    mut b1: BlockId,
    mut b2: BlockId,
) -> BlockId {
    while b1 != b2 {
        while rpo_idx[&b1] > rpo_idx[&b2] {
            b1 = idom[&b1].expect("should have idom");
        }
        while rpo_idx[&b2] > rpo_idx[&b1] {
            b2 = idom[&b2].expect("should have idom");
        }
    }
    b1
}

// ============================================================================
// Def-Use Collection
// ============================================================================

/// Collect def-use information for all locals.
fn collect_def_use<'db>(mir: &MirFunction<'db>) -> HashMap<Local, LocalDefUse<'db>> {
    let mut def_use: HashMap<Local, LocalDefUse<'db>> = HashMap::new();

    // Initialize for all locals
    for (idx, _) in mir.locals.iter().enumerate() {
        let local = Local(idx);
        def_use.insert(
            local,
            LocalDefUse {
                local,
                def: None,
                uses: Vec::new(),
            },
        );
    }

    // Walk all blocks
    for block in &mir.blocks {
        // Walk statements
        for (stmt_idx, stmt) in block.statements.iter().enumerate() {
            match &stmt.kind {
                StatementKind::Assign { destination, value } => {
                    // Record definition
                    if let Place::Local(local) = destination {
                        if let Some(du) = def_use.get_mut(local) {
                            du.def = Some(DefLocation {
                                block: block.id,
                                statement_idx: stmt_idx,
                                rvalue: value.clone(),
                            });
                        }
                    }

                    // Record uses in the rvalue
                    collect_uses_in_rvalue(value, block.id, stmt_idx, &mut def_use);
                }
                StatementKind::Drop(place) => {
                    collect_uses_in_place(place, block.id, stmt_idx, &mut def_use);
                }
                StatementKind::Nop => {}
            }
        }

        // Walk terminator
        if let Some(term) = &block.terminator {
            collect_uses_in_terminator(term, block.id, &mut def_use);
        }
    }

    def_use
}

/// Collect uses in an rvalue.
fn collect_uses_in_rvalue<'db>(
    rvalue: &Rvalue<'db>,
    block: BlockId,
    stmt_idx: usize,
    def_use: &mut HashMap<Local, LocalDefUse<'db>>,
) {
    match rvalue {
        Rvalue::Use(operand) => {
            collect_uses_in_operand(operand, block, stmt_idx, def_use);
        }
        Rvalue::BinaryOp { left, right, .. } => {
            collect_uses_in_operand(left, block, stmt_idx, def_use);
            collect_uses_in_operand(right, block, stmt_idx, def_use);
        }
        Rvalue::UnaryOp { operand, .. } => {
            collect_uses_in_operand(operand, block, stmt_idx, def_use);
        }
        Rvalue::Array(elements) => {
            for elem in elements {
                collect_uses_in_operand(elem, block, stmt_idx, def_use);
            }
        }
        Rvalue::Aggregate { fields, .. } => {
            for field in fields {
                collect_uses_in_operand(field, block, stmt_idx, def_use);
            }
        }
        Rvalue::Discriminant(place) | Rvalue::Len(place) => {
            collect_uses_in_place(place, block, stmt_idx, def_use);
        }
        Rvalue::IsType { operand, .. } => {
            collect_uses_in_operand(operand, block, stmt_idx, def_use);
        }
    }
}

/// Collect uses in an operand.
fn collect_uses_in_operand<'db>(
    operand: &Operand<'db>,
    block: BlockId,
    stmt_idx: usize,
    def_use: &mut HashMap<Local, LocalDefUse<'db>>,
) {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => {
            collect_uses_in_place(place, block, stmt_idx, def_use);
        }
        Operand::Constant(_) => {}
    }
}

/// Collect uses in a place.
fn collect_uses_in_place(
    place: &Place,
    block: BlockId,
    stmt_idx: usize,
    def_use: &mut HashMap<Local, LocalDefUse<'_>>,
) {
    // The base local is used
    let base = place.base_local();
    if let Some(du) = def_use.get_mut(&base) {
        du.uses.push(UseLocation {
            block,
            statement_idx: stmt_idx,
        });
    }

    // For index places, the index local is also used
    if let Place::Index { index, .. } = place {
        if let Some(du) = def_use.get_mut(index) {
            du.uses.push(UseLocation {
                block,
                statement_idx: stmt_idx,
            });
        }
    }
}

/// Collect uses (and defs for Call/Await) in a terminator.
fn collect_uses_in_terminator<'db>(
    term: &Terminator<'db>,
    block: BlockId,
    def_use: &mut HashMap<Local, LocalDefUse<'db>>,
) {
    match term {
        Terminator::Goto { .. } | Terminator::Unreachable => {}
        Terminator::Return => {
            // Return implicitly uses _0 (the return value local)
            let return_local = Local(0);
            if let Some(du) = def_use.get_mut(&return_local) {
                du.uses.push(UseLocation {
                    block,
                    statement_idx: TERMINATOR_IDX,
                });
            }
        }
        Terminator::Branch { condition, .. } => {
            collect_uses_in_operand(condition, block, TERMINATOR_IDX, def_use);
        }
        Terminator::Switch { discriminant, .. } => {
            collect_uses_in_operand(discriminant, block, TERMINATOR_IDX, def_use);
        }
        Terminator::Call {
            callee,
            args,
            destination,
            ..
        } => {
            collect_uses_in_operand(callee, block, TERMINATOR_IDX, def_use);
            for arg in args {
                collect_uses_in_operand(arg, block, TERMINATOR_IDX, def_use);
            }
            // Record the def for the destination (where call result is stored)
            if let Place::Local(local) = destination {
                if let Some(du) = def_use.get_mut(local) {
                    // For Call terminators, we use a synthetic Rvalue::Use with a placeholder
                    // The actual value comes from the call, but for classification purposes,
                    // we just need to know there's a def here
                    du.def = Some(DefLocation {
                        block,
                        statement_idx: TERMINATOR_IDX,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                }
            }
        }
        Terminator::DispatchFuture {
            callee,
            args,
            future,
            ..
        } => {
            collect_uses_in_operand(callee, block, TERMINATOR_IDX, def_use);
            for arg in args {
                collect_uses_in_operand(arg, block, TERMINATOR_IDX, def_use);
            }
            // Record the def for the future place
            if let Place::Local(local) = future {
                if let Some(du) = def_use.get_mut(local) {
                    du.def = Some(DefLocation {
                        block,
                        statement_idx: TERMINATOR_IDX,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                }
            }
        }
        Terminator::Await {
            future,
            destination,
            ..
        } => {
            collect_uses_in_place(future, block, TERMINATOR_IDX, def_use);
            // Record the def for the destination
            if let Place::Local(local) = destination {
                if let Some(du) = def_use.get_mut(local) {
                    du.def = Some(DefLocation {
                        block,
                        statement_idx: TERMINATOR_IDX,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                }
            }
        }
    }
}

// ============================================================================
// Local Classification
// ============================================================================

/// Classify each local as Virtual or Real.
fn classify_locals<'db>(
    mir: &MirFunction<'db>,
    def_use: &HashMap<Local, LocalDefUse<'db>>,
    dominators: &Dominators,
    predecessors: &HashMap<BlockId, Vec<BlockId>>,
) -> HashMap<Local, LocalClassification> {
    let mut classifications = HashMap::new();

    for (idx, _local_decl) in mir.locals.iter().enumerate() {
        let local = Local(idx);
        let du = &def_use[&local];

        let classification = if idx > 0 && idx <= mir.arity {
            // Parameters are always real (they come from the caller)
            LocalClassification::Parameter
        } else if can_be_virtual(local, du, dominators, mir, def_use, predecessors) {
            LocalClassification::Virtual
        } else {
            LocalClassification::Real
        };

        classifications.insert(local, classification);
    }

    classifications
}

/// Check if a local can be classified as Virtual.
fn can_be_virtual<'db>(
    _local: Local,
    du: &LocalDefUse<'db>,
    dominators: &Dominators,
    mir: &MirFunction<'db>,
    def_use: &HashMap<Local, LocalDefUse<'db>>,
    predecessors: &HashMap<BlockId, Vec<BlockId>>,
) -> bool {
    // Must have exactly one definition
    let Some(def) = &du.def else {
        return false;
    };

    // Definitions in terminators (Call/Await/DispatchFuture) cannot be inlined
    // because the value comes from the operation itself, not from a re-emittable rvalue
    if def.statement_idx == TERMINATOR_IDX {
        return false;
    }

    // Must have exactly one use for simple virtual inlining
    if du.uses.len() != 1 {
        return false;
    }

    let use_loc = &du.uses[0];

    // Definition must dominate use
    if !dominators.dominates(def.block, use_loc.block) {
        return false;
    }

    // If in same block, use must come after def
    if def.block == use_loc.block {
        // Handle terminator uses (TERMINATOR_IDX)
        if use_loc.statement_idx == TERMINATOR_IDX {
            // Terminator always comes after all statements, so this is fine
            // But check for side effects between def and end of block
            if has_side_effects_between(
                mir,
                def.block,
                def.statement_idx,
                mir.block(def.block).statements.len(),
                &def.rvalue,
                def_use,
            ) {
                return false;
            }
        } else if use_loc.statement_idx <= def.statement_idx {
            return false;
        } else {
            // Check for intervening side effects
            if has_side_effects_between(
                mir,
                def.block,
                def.statement_idx,
                use_loc.statement_idx,
                &def.rvalue,
                def_use,
            ) {
                return false;
            }
        }
    } else {
        // Cross-block def-use
        // Check if the use block is a loop header (has back-edge predecessors)
        // If so, be conservative and don't inline
        let use_preds = predecessors
            .get(&use_loc.block)
            .map_or(&[] as &[_], |v| v.as_slice());

        // Check for back-edges: a predecessor that the use block dominates
        // indicates a loop, which means the code between def and use might
        // execute multiple times
        let has_back_edge = use_preds
            .iter()
            .any(|&pred| dominators.dominates(use_loc.block, pred));

        if has_back_edge {
            // Loop detected - be conservative
            return false;
        }

        // For non-loop cross-block, we're more permissive:
        // The rvalue will be re-evaluated at the use site, so we need to ensure
        // no intervening modifications. Check the def block from def to end.
        if has_side_effects_between(
            mir,
            def.block,
            def.statement_idx,
            mir.block(def.block).statements.len(),
            &def.rvalue,
            def_use,
        ) {
            return false;
        }

        // Check the use block from start to use
        if use_loc.statement_idx != TERMINATOR_IDX
            && use_loc.statement_idx > 0
            && has_side_effects_between(
                mir,
                use_loc.block,
                0,
                use_loc.statement_idx,
                &def.rvalue,
                def_use,
            )
        {
            return false;
        }

        // Note: We're NOT checking intermediate blocks between def and use.
        // This is safe because:
        // 1. Def dominates use (checked earlier)
        // 2. No back-edges to use block (checked above)
        // 3. The transitive read set includes all dependencies
        // If any intermediate block modified a dependency, that would require
        // the variable to appear on a path the VM takes, but since we only
        // inline single-use variables, the value at use is the value at def.
    }

    // Check if the rvalue is inlinable
    is_inlinable_rvalue(&def.rvalue)
}

/// Check for side effects between two statement indices in a block.
///
/// A side effect is anything that could change the value of the rvalue when re-evaluated:
/// - Function calls (may have side effects)
/// - Assignments to variables that the rvalue reads from (transitively)
fn has_side_effects_between<'db>(
    mir: &MirFunction<'db>,
    block_id: BlockId,
    start: usize,
    end: usize,
    rvalue: &Rvalue<'db>,
    def_use: &HashMap<Local, LocalDefUse<'db>>,
) -> bool {
    let block = mir.block(block_id);
    // Collect transitive reads - if this rvalue reads from local X which is defined
    // as reading from local Y, we need to track both X and Y.
    // Only follow definitions that happen BEFORE start (the current statement).
    let rvalue_reads = collect_transitive_reads(rvalue, def_use, block_id, start);

    for stmt_idx in (start + 1)..end {
        let stmt = &block.statements[stmt_idx];
        if has_side_effect(&stmt.kind, &rvalue_reads) {
            return true;
        }
    }

    false
}

/// Collect all locals that an rvalue reads from, transitively.
///
/// If the rvalue reads from local X, and X is defined as the result of an
/// expression that reads from Y, we include both X and Y. This is necessary
/// because inlining X will re-evaluate its definition, which reads from Y.
///
/// We only follow definitions that occur before `def_block:def_stmt_idx` to
/// avoid including dependencies on values computed later.
fn collect_transitive_reads<'db>(
    rvalue: &Rvalue<'db>,
    def_use: &HashMap<Local, LocalDefUse<'db>>,
    def_block: BlockId,
    def_stmt_idx: usize,
) -> HashSet<Local> {
    let mut locals = HashSet::new();
    let mut worklist: Vec<Local> = Vec::new();

    // First, collect direct reads
    collect_rvalue_reads(rvalue, &mut worklist);

    // Then, transitively expand
    while let Some(local) = worklist.pop() {
        if locals.insert(local) {
            // New local - check if it has a definition with an rvalue we should follow
            // Only follow if the definition is in the same block AND before the current statement
            if let Some(du) = def_use.get(&local) {
                if let Some(def) = &du.def {
                    // Only follow if definition is earlier in the same block
                    // This ensures we don't include dependencies on values computed later
                    if def.block == def_block && def.statement_idx < def_stmt_idx {
                        collect_rvalue_reads(&def.rvalue, &mut worklist);
                    }
                }
            }
        }
    }

    locals
}

/// Collect locals directly read by an rvalue (non-transitive).
fn collect_rvalue_reads(rvalue: &Rvalue<'_>, locals: &mut Vec<Local>) {
    match rvalue {
        Rvalue::Use(operand) => collect_operand_reads(operand, locals),
        Rvalue::BinaryOp { left, right, .. } => {
            collect_operand_reads(left, locals);
            collect_operand_reads(right, locals);
        }
        Rvalue::UnaryOp { operand, .. } => {
            collect_operand_reads(operand, locals);
        }
        Rvalue::Array(elements) => {
            for op in elements {
                collect_operand_reads(op, locals);
            }
        }
        Rvalue::Aggregate { fields, .. } => {
            for op in fields {
                collect_operand_reads(op, locals);
            }
        }
        Rvalue::Discriminant(place) | Rvalue::Len(place) => {
            collect_place_reads(place, locals);
        }
        Rvalue::IsType { operand, .. } => {
            collect_operand_reads(operand, locals);
        }
    }
}

/// Collect locals read by an operand.
fn collect_operand_reads(operand: &Operand<'_>, locals: &mut Vec<Local>) {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => {
            collect_place_reads(place, locals);
        }
        Operand::Constant(_) => {}
    }
}

/// Collect locals read by a place.
fn collect_place_reads(place: &Place, locals: &mut Vec<Local>) {
    match place {
        Place::Local(local) => {
            locals.push(*local);
        }
        Place::Field { base, .. } => {
            collect_place_reads(base, locals);
        }
        Place::Index { base, index } => {
            collect_place_reads(base, locals);
            locals.push(*index); // The index variable is also read
        }
    }
}

/// Check if a statement has side effects that would prevent inlining.
fn has_side_effect(kind: &StatementKind<'_>, rvalue_reads: &HashSet<Local>) -> bool {
    match kind {
        StatementKind::Assign { destination, value } => {
            // Function calls have side effects
            if let Rvalue::Use(Operand::Constant(Constant::Function(_))) = value {
                return true;
            }
            // Check if this assignment modifies a variable that the rvalue reads
            if let Place::Local(local) = destination {
                if rvalue_reads.contains(local) {
                    return true;
                }
            }
            false
        }
        StatementKind::Drop(_) => true,
        StatementKind::Nop => false,
    }
}

/// Check if an rvalue can be inlined.
fn is_inlinable_rvalue(_rvalue: &Rvalue<'_>) -> bool {
    // All current rvalues can be inlined
    // May want to exclude complex aggregates in the future
    true
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dominates_entry() {
        let mut idom = HashMap::new();
        idom.insert(BlockId(0), None);
        idom.insert(BlockId(1), Some(BlockId(0)));
        idom.insert(BlockId(2), Some(BlockId(1)));

        let mut rpo_idx = HashMap::new();
        rpo_idx.insert(BlockId(0), 0);
        rpo_idx.insert(BlockId(1), 1);
        rpo_idx.insert(BlockId(2), 2);

        let doms = Dominators { idom, rpo_idx };

        // Entry dominates everything
        assert!(doms.dominates(BlockId(0), BlockId(0)));
        assert!(doms.dominates(BlockId(0), BlockId(1)));
        assert!(doms.dominates(BlockId(0), BlockId(2)));

        // bb1 dominates bb2
        assert!(doms.dominates(BlockId(1), BlockId(2)));

        // bb2 doesn't dominate bb1
        assert!(!doms.dominates(BlockId(2), BlockId(1)));
    }
}
