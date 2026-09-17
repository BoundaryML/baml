//! MIR analysis for stackification.
//!
//! This module provides:
//! - CFG predecessor computation
//! - Dominator tree computation (Cooper-Harvey-Kennedy algorithm)
//! - Def-use information collection, over locals and over the frame type-arg
//!   slots an rvalue's templates read (rebound by `BindType`)
//! - Local classification (Virtual vs Real), from one rematerialization
//!   judgment: an evaluation may move to its use when it is repeatable and
//!   nothing it reads (see `baml_compiler2_mir::memory`) is written on any
//!   path from its definition to that use
//! - Jump threading (redirect targets for empty goto-only blocks)
//! - Phi-like local detection (locals assigned in all predecessors, used once at join)
//! - Constant propagation (pure constants with single definition inlined at all use sites)
//! - Call result immediate (single-use Call results used at continuation block start)
//! - Copy propagation (locals that are simple copies of parameters/other locals)
//! - Wildcard elimination (unused `_` pattern bindings are eliminated)

use std::collections::{HashMap, HashSet};

pub use baml_compiler2_mir::OptLevel;
use baml_compiler2_mir::{
    BlockId, Constant, IntrinsicOp, Local, MirFunctionBody, Operand, Place, Rvalue, StatementKind,
    Terminator, memory,
};
use memory::Resources;

use crate::stack_carry;

// ============================================================================
// Data Structures
// ============================================================================

/// A reference to either a statement or a terminator within a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum StatementRef {
    /// A statement at the given index.
    Statement(usize),
    /// The block's terminator.
    Terminator,
}

/// Where a local is defined.
#[derive(Clone, Debug)]
pub(crate) struct DefLocation<'db> {
    pub block: BlockId,
    pub statement_ref: StatementRef,
    /// The rvalue that produces this local's value (for inlining).
    pub rvalue: Rvalue<'db>,
}

/// Where a local is used.
#[derive(Clone, Debug)]
pub(crate) struct UseLocation {
    pub block: BlockId,
    pub statement_ref: StatementRef,
}

/// Def-use information for a single local.
#[derive(Clone, Debug)]
pub(crate) struct LocalDefUse<'db> {
    /// Definition site (None for parameters, which are defined at entry).
    pub def: Option<DefLocation<'db>>,
    /// All use sites.
    pub uses: Vec<UseLocation>,
    /// All definition sites as `(block, statement_ref)` pairs.
    /// Empty for parameters that are never reassigned.
    pub all_defs: Vec<(BlockId, StatementRef)>,
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
    /// Phi-like local: assigned in each predecessor of a join block, used once at join.
    /// At def sites: emit rvalue but NOT store (leave on stack).
    /// At use site: don't emit `LoadVar` (value already on stack from predecessor).
    PhiLike,
    /// Return-phi: _0 is assigned immediately before Return in each defining block.
    /// At def sites: emit rvalue but NOT store (leave on stack).
    /// At Return: don't emit `LoadVar` for _0 (value already on stack).
    ReturnPhi,
    /// Call result immediate: defined by Call/VirtualCall/Await/SysOp, used exactly once
    /// immediately in the continuation block.
    /// At def site (after Call): don't emit Store (leave on stack).
    /// At use site: don't emit `LoadVar` (value already on stack from Call).
    CallResultImmediate,
    /// A value carried as part of an aggregate, binary, or call operand prefix.
    ///
    /// At def site: don't store the result; leave it on the stack.
    /// At use site: don't emit `LoadVar`; the operation consumes the
    /// already-stacked value in operand order.
    AggregateOperand,
    /// Copy of another local: `_X = copy _Y` where _Y is a parameter or simple local.
    /// At def site: don't emit anything (skip the copy entirely).
    /// At use sites: load from the source local instead.
    /// The source local is stored in `AnalysisResult::copy_sources`.
    CopyOf,
    /// Dead local - defined but never used, can be eliminated.
    Dead,
}

/// Dominator tree.
#[derive(Debug)]
pub(crate) struct Dominators {
    /// Immediate dominator of each block (entry has None).
    pub idom: HashMap<BlockId, Option<BlockId>>,
    /// Reverse postorder indices used by `intersect()` during dominator computation.
    /// The `dead_code` lint fires because the field is only read via a free function,
    /// not through a method on `Dominators`.
    #[allow(dead_code)]
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
    /// Reverse postorder of blocks (for iteration).
    pub rpo: Vec<BlockId>,
    /// Jump threading: maps empty goto-only blocks to their final target.
    /// Used during emission to skip intermediate jumps.
    pub redirect_targets: HashMap<BlockId, BlockId>,
    /// Copy propagation: maps locals classified as `CopyOf` to their source local.
    /// When emitting a use of local X, if X is in this map, load from the mapped local instead.
    pub copy_sources: HashMap<Local, Local>,
}

// ============================================================================
// Analysis Entry Point
// ============================================================================

impl<'db> AnalysisResult<'db> {
    /// Analyze a MIR function and produce classification results.
    pub(crate) fn analyze(body: &MirFunctionBody<'db>, arity: usize, opt: OptLevel) -> Self {
        // Step 1: Build predecessor map
        let predecessors = build_predecessors(body);
        let trap_cfg = TrapCfg::new(body);

        // Step 2: Compute reverse postorder
        let rpo = compute_rpo(body);

        // Step 3: Compute dominators
        let dominators = compute_dominators(body, &rpo, &predecessors);
        #[cfg(debug_assertions)]
        assert_rebound_slots_dominate_their_reads(body);

        // Step 4: Collect def-use information
        let def_use = collect_def_use(body);

        // Step 5: Conservative jump threading (truly empty goto-only blocks).
        let initial_redirect_targets = build_redirect_targets(body);

        // Step 6: What every instruction writes, once.
        let clobbers = Clobbers::new(body);
        let cx = ClassifyCx {
            body,
            arity,
            def_use: &def_use,
            dominators: &dominators,
            predecessors: &predecessors,
            trap_cfg: &trap_cfg,
            clobbers: &clobbers,
            track_type_slots: body_rebinds_type_slots(body),
            opt,
        };

        // Step 7: First classification pass.
        let (mut classifications, mut copy_sources) =
            classify_locals(&cx, &initial_redirect_targets);

        // Step 8: Enhanced jump threading using classification info.
        // Some blocks have statements that produce no bytecode (Virtual, Dead,
        // CopyOf assignments). These are effectively empty and can be threaded.
        let redirect_targets = build_redirect_targets_with_classifications(body, &classifications);

        // Step 9: Re-run classification once if redirects changed.
        // `ReturnPhi` checks walk through redirects, so this lets classification
        // observe the final threaded CFG without requiring a general fixpoint loop.
        //
        // NOTE: This bounded refinement is sufficient for the current pipeline because
        // redirect construction only depends on `Virtual | Dead | CopyOf`, which are
        // not redirect-sensitive today. If future MIR optimizations introduce feedback
        // where redirect-sensitive classifications can make blocks newly threadable
        // (or iterative transforms like branch folding/DCE rewrite CFG edges between
        // rounds), upgrade this to a true fixed-point convergence loop.
        if redirect_targets != initial_redirect_targets {
            let (reclassified, recopy_sources) = classify_locals(&cx, &redirect_targets);
            classifications = reclassified;
            copy_sources = recopy_sources;
        }

        Self {
            classifications,
            def_use,
            rpo,
            redirect_targets,
            copy_sources,
        }
    }

    /// Resolve a jump target through the redirect map.
    /// Returns the final target after following any redirect chains.
    pub(crate) fn resolve_jump_target(&self, target: BlockId) -> BlockId {
        self.redirect_targets
            .get(&target)
            .copied()
            .unwrap_or(target)
    }

    /// Resolve a local through copy propagation.
    /// If the local is a copy of another local, returns the source local.
    /// Follows chains: if A copies B and B copies C, resolves A to C.
    pub(crate) fn resolve_copy_source(&self, local: Local) -> Local {
        let mut current = local;
        while let Some(&source) = self.copy_sources.get(&current) {
            current = source;
        }
        current
    }
}

// ============================================================================
// CFG Analysis
// ============================================================================

/// Build predecessor map for all blocks over the terminator edges.
fn build_predecessors(body: &MirFunctionBody<'_>) -> HashMap<BlockId, Vec<BlockId>> {
    build_predecessors_with(body, &|block| successors_of(body, block))
}

/// The catch handler(s) each protected block can land in.
///
/// A throwing call reaches its handler through the terminator's `unwind`
/// edge, but a call-free trap (division, indexing) reaches it through the
/// exception table with no CFG edge at all. Any walk that must see every
/// path a value can take from a definition to a use has to add these edges.
fn catch_handlers_of(body: &MirFunctionBody<'_>) -> HashMap<BlockId, Vec<BlockId>> {
    let mut handlers_of: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
    for region in &body.catch_regions {
        for &protected in &region.body_blocks {
            handlers_of
                .entry(protected)
                .or_default()
                .push(region.handler);
        }
    }
    handlers_of
}

/// The terminator successors of `block` plus the handlers a trap inside it
/// can land in (see [`catch_handlers_of`]).
fn successors_with_handlers(
    body: &MirFunctionBody<'_>,
    handlers_of: &HashMap<BlockId, Vec<BlockId>>,
    block: BlockId,
) -> Vec<BlockId> {
    let mut successors = successors_of(body, block);
    if let Some(handlers) = handlers_of.get(&block) {
        successors.extend(handlers.iter().copied());
    }
    successors
}

/// The CFG a def-to-use path walk must use: the terminator edges plus the trap
/// edges into catch handlers (see [`catch_handlers_of`]). A trap is a real path
/// that the terminator edges alone do not show, so a walk over them alone can
/// miss a clobber sitting in a handler.
struct TrapCfg {
    handlers_of: HashMap<BlockId, Vec<BlockId>>,
    predecessors: HashMap<BlockId, Vec<BlockId>>,
    /// The catch regions each block's code is protected by, as sorted region
    /// indices: two blocks with the same list unwind to the same handler.
    regions_of: HashMap<BlockId, Vec<usize>>,
}

impl TrapCfg {
    fn new(body: &MirFunctionBody<'_>) -> Self {
        let handlers_of = catch_handlers_of(body);
        let predecessors = build_predecessors_with(body, &|block| {
            successors_with_handlers(body, &handlers_of, block)
        });
        let mut regions_of: HashMap<BlockId, Vec<usize>> = HashMap::new();
        for (index, region) in body.catch_regions.iter().enumerate() {
            for &block in &region.body_blocks {
                regions_of.entry(block).or_default().push(index);
            }
        }
        Self {
            handlers_of,
            predecessors,
            regions_of,
        }
    }

    /// Whether a trap in `a` and a trap in `b` reach the same handler.
    fn same_handler(&self, a: BlockId, b: BlockId) -> bool {
        self.regions_of.get(&a).map_or(&[][..], Vec::as_slice)
            == self.regions_of.get(&b).map_or(&[][..], Vec::as_slice)
    }

    fn successors(&self, body: &MirFunctionBody<'_>, block: BlockId) -> Vec<BlockId> {
        successors_with_handlers(body, &self.handlers_of, block)
    }

    fn predecessors(&self, block: BlockId) -> Vec<BlockId> {
        self.predecessors.get(&block).cloned().unwrap_or_default()
    }
}

/// Build predecessor map for all blocks over the edges `successors` yields.
fn build_predecessors_with(
    body: &MirFunctionBody<'_>,
    successors: &impl Fn(BlockId) -> Vec<BlockId>,
) -> HashMap<BlockId, Vec<BlockId>> {
    let mut preds: HashMap<BlockId, Vec<BlockId>> = HashMap::new();

    // Initialize with empty vecs
    for block in &body.blocks {
        preds.insert(block.id, Vec::new());
    }

    for block in &body.blocks {
        for succ in successors(block.id) {
            if let Some(pred_list) = preds.get_mut(&succ) {
                pred_list.push(block.id);
            }
        }
    }

    preds
}

/// DFS helper for computing postorder.
fn rpo_dfs(
    successors: &impl Fn(BlockId) -> Vec<BlockId>,
    block_id: BlockId,
    visited: &mut HashSet<BlockId>,
    postorder: &mut Vec<BlockId>,
) {
    if visited.contains(&block_id) {
        return;
    }
    visited.insert(block_id);

    for succ in successors(block_id) {
        rpo_dfs(successors, succ, visited, postorder);
    }
    postorder.push(block_id);
}

/// Compute reverse postorder (depth-first, postorder reversed) over the
/// terminator edges.
fn compute_rpo(body: &MirFunctionBody<'_>) -> Vec<BlockId> {
    compute_rpo_with(body, &|block| successors_of(body, block))
}

/// Compute reverse postorder over the edges `successors` yields.
fn compute_rpo_with(
    body: &MirFunctionBody<'_>,
    successors: &impl Fn(BlockId) -> Vec<BlockId>,
) -> Vec<BlockId> {
    let mut visited = HashSet::new();
    let mut postorder = Vec::new();

    // Phase 1: DFS from the entry block. Handlers reachable via CFG edges
    // (Call/Await unwind targets) are visited as descendants of their
    // try-body entry blocks. Layout order does not affect exception-table
    // correctness (the table lists each region's protected blocks' exact PC
    // ranges), so this is purely about code locality and readability.
    rpo_dfs(successors, body.entry, &mut visited, &mut postorder);

    // Phase 2: Seed handlers NOT reachable from entry (same-frame panics
    // like division-by-zero where there's no Call/Await with an unwind
    // edge) so they are emitted at all; they land after all entry-reachable
    // blocks in the reversed RPO.
    let mut handler_postorder = Vec::new();
    for region in &body.catch_regions {
        rpo_dfs(
            successors,
            region.handler,
            &mut visited,
            &mut handler_postorder,
        );
    }
    handler_postorder.append(&mut postorder);

    handler_postorder.reverse();
    handler_postorder
}

// ============================================================================
// Emission Helpers
// ============================================================================

/// Check if a block is a "dead" unreachable block that may be skipped during
/// emission without changing observable behavior.
///
/// A block is dead if it has no statements and terminates with `Unreachable`.
pub(crate) fn is_dead_unreachable_block(block: &baml_compiler2_mir::BasicBlock) -> bool {
    block.statements.is_empty() && matches!(block.terminator, Some(Terminator::Unreachable))
}

// ============================================================================
// Jump Threading
// ============================================================================

/// Build redirect targets for jump threading.
///
/// Identifies empty blocks that only contain a Goto terminator and maps them
/// to their final destination. This allows emission to skip intermediate jumps.
fn build_redirect_targets(body: &MirFunctionBody<'_>) -> HashMap<BlockId, BlockId> {
    // First pass: identify empty goto-only blocks
    let mut goto_targets: HashMap<BlockId, BlockId> = HashMap::new();

    for block in &body.blocks {
        if block.statements.is_empty() {
            if let Some(Terminator::Goto { target }) = &block.terminator {
                goto_targets.insert(block.id, *target);
            }
        }
    }

    // Second pass: resolve chains (A -> B -> C becomes A -> C)
    let mut resolved: HashMap<BlockId, BlockId> = HashMap::new();

    for &block_id in goto_targets.keys() {
        let final_target = resolve_redirect_chain(block_id, &goto_targets);
        // Only add to resolved if there's actually a redirect
        if final_target != block_id {
            resolved.insert(block_id, final_target);
        }
    }

    resolved
}

/// Follow a chain of redirects to find the final target.
fn resolve_redirect_chain(start: BlockId, goto_targets: &HashMap<BlockId, BlockId>) -> BlockId {
    let mut current = start;
    let mut visited = HashSet::new();

    while let Some(&next) = goto_targets.get(&current) {
        // Avoid infinite loops (shouldn't happen in well-formed MIR)
        if !visited.insert(current) {
            break;
        }
        current = next;
    }

    current
}

/// Build redirect targets using local classification info.
///
/// Like [`build_redirect_targets`] but also threads through blocks whose
/// statements all target locals classified as [`LocalClassification::Virtual`],
/// [`LocalClassification::Dead`], or [`LocalClassification::CopyOf`]. These
/// assignments produce no bytecode during emission, making the block
/// effectively empty.
fn build_redirect_targets_with_classifications(
    body: &MirFunctionBody<'_>,
    classifications: &HashMap<Local, LocalClassification>,
) -> HashMap<BlockId, BlockId> {
    let mut goto_targets: HashMap<BlockId, BlockId> = HashMap::new();

    for block in &body.blocks {
        if let Some(target) = threadable_goto_target(block, classifications) {
            goto_targets.insert(block.id, target);
        }
    }

    // Resolve chains (A -> B -> C becomes A -> C).
    let mut resolved: HashMap<BlockId, BlockId> = HashMap::new();

    for &block_id in goto_targets.keys() {
        let final_target = resolve_redirect_chain(block_id, &goto_targets);
        if final_target != block_id {
            resolved.insert(block_id, final_target);
        }
    }

    resolved
}

/// Return the goto target if this block is threadable as an effectively-empty
/// redirect source under the given local classifications.
pub(crate) fn threadable_goto_target(
    block: &baml_compiler2_mir::BasicBlock,
    classifications: &HashMap<Local, LocalClassification>,
) -> Option<BlockId> {
    let Some(Terminator::Goto { target }) = &block.terminator else {
        return None;
    };

    let effectively_empty = block.statements.iter().all(|stmt| {
        matches!(
            &stmt.kind,
            StatementKind::Assign {
                destination: Place::Local(local),
                ..
            } if matches!(
                classifications.get(local),
                Some(
                    LocalClassification::Virtual
                    | LocalClassification::Dead
                    | LocalClassification::CopyOf
                )
            )
        )
    });

    effectively_empty.then_some(*target)
}

// ============================================================================
// Dominator Computation (Cooper-Harvey-Kennedy Algorithm)
// ============================================================================

/// Compute dominators using the Cooper-Harvey-Kennedy algorithm.
///
/// This is a simple, efficient iterative algorithm that computes immediate
/// dominators by repeatedly intersecting dominator sets until convergence.
fn compute_dominators(
    body: &MirFunctionBody<'_>,
    rpo: &[BlockId],
    preds: &HashMap<BlockId, Vec<BlockId>>,
) -> Dominators {
    // Map BlockId to RPO index for faster lookup
    let rpo_idx: HashMap<BlockId, usize> = rpo.iter().enumerate().map(|(i, &b)| (b, i)).collect();

    let mut idom: HashMap<BlockId, Option<BlockId>> = HashMap::new();

    // Initialize: entry dominates itself (represented as None for "no parent")
    idom.insert(body.entry, None);

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
fn collect_def_use<'db>(body: &MirFunctionBody<'db>) -> HashMap<Local, LocalDefUse<'db>> {
    let mut def_use: HashMap<Local, LocalDefUse<'db>> = HashMap::new();

    // Initialize for all locals
    for (idx, _) in body.locals.iter().enumerate() {
        let local = Local(idx);
        def_use.insert(
            local,
            LocalDefUse {
                def: None,
                uses: Vec::new(),
                all_defs: Vec::new(),
            },
        );
    }

    // Walk all blocks
    for block in &body.blocks {
        // Walk statements
        for (stmt_idx, stmt) in block.statements.iter().enumerate() {
            let stmt_ref = StatementRef::Statement(stmt_idx);
            match &stmt.kind {
                StatementKind::Assign { destination, value } => {
                    // Record definition
                    if let Place::Local(local) = destination {
                        if let Some(du) = def_use.get_mut(local) {
                            du.def = Some(DefLocation {
                                block: block.id,
                                statement_ref: stmt_ref,
                                rvalue: value.clone(),
                            });
                            du.all_defs.push((block.id, stmt_ref));
                        }
                    }

                    // For field/index stores, the base local (and index local for Index) is also
                    // used. We need to load them to store the value. This ensures they aren't
                    // classified as Virtual.
                    match destination {
                        Place::Field { base, .. } => {
                            collect_uses_in_place(base, block.id, stmt_ref, &mut def_use);
                        }
                        Place::Index { base, index, .. } => {
                            collect_uses_in_place(base, block.id, stmt_ref, &mut def_use);
                            // The index is also used - we need to load it for the Store*Element
                            def_use.get_mut(index).unwrap().uses.push(UseLocation {
                                block: block.id,
                                statement_ref: stmt_ref,
                            });
                        }
                        Place::Local(_) => {}
                        // A store through a cell reads the pointer local.
                        Place::Deref(_) => {
                            collect_uses_in_place(destination, block.id, stmt_ref, &mut def_use);
                        }
                        Place::Capture(_) => {
                            unreachable!("a bare capture is a pointer nothing stores to")
                        }
                    }

                    // Record uses in the rvalue
                    collect_uses_in_rvalue(value, block.id, stmt_ref, &mut def_use);
                }
                StatementKind::VirtualFieldStore {
                    receiver, value, ..
                } => {
                    collect_uses_in_operand(receiver, block.id, stmt_ref, &mut def_use);
                    collect_uses_in_operand(value, block.id, stmt_ref, &mut def_use);
                }
                StatementKind::Drop(place) => {
                    collect_uses_in_place(place, block.id, stmt_ref, &mut def_use);
                }
                StatementKind::Intrinsic { args, .. } => {
                    // Intrinsic args are reads — record uses for each operand
                    for arg in args {
                        collect_uses_in_operand(arg, block.id, stmt_ref, &mut def_use);
                    }
                }
                StatementKind::FreshCell { local, carry_value } => {
                    // A definition of the pointer slot (it now holds a new
                    // cell); carrying also reads the cell being replaced.
                    let du = def_use.get_mut(local).unwrap();
                    du.all_defs.push((block.id, stmt_ref));
                    if *carry_value {
                        du.uses.push(UseLocation {
                            block: block.id,
                            statement_ref: stmt_ref,
                        });
                    }
                }
                StatementKind::Nop => {}
            }
        }

        // Walk terminator
        if let Some(term) = &block.terminator {
            collect_uses_in_terminator(term, block.id, &mut def_use);
        }
    }

    // Unwind error locals are implicitly used by the exception table —
    // the VM writes into these slots when an exception is caught. Without
    // this, the locals may have zero recorded uses and get classified Dead,
    // causing a panic when the emitter tries to allocate a slot for them.
    for (block_id, local) in body.unwind_error_locals() {
        if let Some(du) = def_use.get_mut(&local) {
            du.uses.push(UseLocation {
                block: block_id,
                statement_ref: StatementRef::Terminator,
            });
        }
    }

    // The VM also materializes the caught error's `baml.errors.Context` into the
    // context (second-binding) slot, and the BEP-042 cause-chain pre-walk reads
    // it from an *enclosing* handler — uses the static walk can't see. Mark it
    // used so it isn't classified Dead and always gets a slot, even when the
    // `ctx` binding looks statically dead.
    for region in &body.catch_regions {
        if let Some(ctx_local) = region.stack_trace_local
            && let Some(du) = def_use.get_mut(&ctx_local)
        {
            du.uses.push(UseLocation {
                block: region.handler,
                statement_ref: StatementRef::Terminator,
            });
        }
    }

    def_use
}

// ---------------------------------------------------------------------------
// Generic local walkers: single source of truth for traversing MIR trees.
// ---------------------------------------------------------------------------

/// Walk all locals referenced by a place expression, calling `f` for each.
fn walk_place_locals(place: &Place, f: &mut impl FnMut(Local)) {
    match place {
        Place::Local(local) => f(*local),
        Place::Deref(cell) => {
            if let Some(local) = cell.local() {
                f(local);
            }
        }
        Place::Capture(_) => {
            // A capture slot is a cell in the closure object, not a local:
            // there is no local-level definition to count. As a resource it
            // is `memory::CellId::Capture`.
        }
        Place::Field { base, .. } => walk_place_locals(base, f),
        Place::Index { base, index, .. } => {
            walk_place_locals(base, f);
            f(*index);
        }
    }
}

/// Walk all locals referenced by an operand, calling `f` for each.
fn walk_operand_locals(operand: &Operand<'_>, f: &mut impl FnMut(Local)) {
    memory::walk_operand_places(operand, &mut |place| walk_place_locals(place, f));
}

/// Walk all locals referenced by an rvalue, calling `f` for each.
fn walk_rvalue_locals(rvalue: &Rvalue<'_>, f: &mut impl FnMut(Local)) {
    memory::walk_rvalue_places(rvalue, &mut |place| walk_place_locals(place, f));
}

/// Debug tripwire for the lowering invariant the cross-block virtualization
/// check leans on: a frame type-arg slot this body rebinds is read only where
/// some `BindType` of it has already run, i.e. one that dominates the read.
/// Slots with no writer in this body belong to the caller (generic arguments,
/// a captured layout) and are outside the check.
///
/// Exception entry is control flow too: a handler runs after any block of its
/// protected region raises. The CFG the emitter lays out carries only the
/// unwind edges of calls — a call-free panic (division, indexing) reaches its
/// handler through the exception table alone — so dominance here is computed
/// over that CFG plus one edge from every protected block to its handler. A
/// block reachable neither way never runs, and its reads are vacuous.
#[cfg(debug_assertions)]
fn assert_rebound_slots_dominate_their_reads(body: &MirFunctionBody<'_>) {
    let mut writers: HashMap<u32, Vec<(BlockId, usize)>> = HashMap::new();
    for block in &body.blocks {
        for (idx, stmt) in block.statements.iter().enumerate() {
            if let StatementKind::Intrinsic {
                op: IntrinsicOp::BindType(slot),
                ..
            } = &stmt.kind
            {
                writers.entry(*slot).or_default().push((block.id, idx));
            }
        }
    }
    if writers.is_empty() {
        return;
    }
    let handlers_of = catch_handlers_of(body);
    let successors = |block: BlockId| successors_with_handlers(body, &handlers_of, block);
    let rpo = compute_rpo_with(body, &successors);
    let predecessors = build_predecessors_with(body, &successors);
    let dominators = compute_dominators(body, &rpo, &predecessors);
    let bound_before = |slot: u32, block: BlockId, position: usize| {
        if !dominators.idom.contains_key(&block) {
            return true;
        }
        writers.get(&slot).is_none_or(|sites| {
            sites.iter().any(|&(writer_block, writer_idx)| {
                if writer_block == block {
                    writer_idx < position
                } else {
                    dominators.dominates(writer_block, block)
                }
            })
        })
    };
    for block in &body.blocks {
        for (idx, stmt) in block.statements.iter().enumerate() {
            memory::walk_statement_type_slots(&stmt.kind, &mut |slot| {
                assert!(
                    bound_before(slot, block.id, idx),
                    "frame type-arg slot {slot} is read at {:?}[{idx}] before any `BindType` of it has run",
                    block.id
                );
            });
        }
        if let Some(terminator) = &block.terminator {
            memory::walk_terminator_type_slots(terminator, &mut |slot| {
                assert!(
                    bound_before(slot, block.id, block.statements.len()),
                    "frame type-arg slot {slot} is read by the terminator of {:?} before any `BindType` of it has run",
                    block.id
                );
            });
        }
    }
}

/// Record a use of every local referenced by an rvalue.
fn collect_uses_in_rvalue(
    rvalue: &Rvalue<'_>,
    block: BlockId,
    stmt_ref: StatementRef,
    def_use: &mut HashMap<Local, LocalDefUse>,
) {
    walk_rvalue_locals(rvalue, &mut |local| {
        if let Some(du) = def_use.get_mut(&local) {
            du.uses.push(UseLocation {
                block,
                statement_ref: stmt_ref,
            });
        }
    });
}

/// Record a use of every local referenced by an operand.
fn collect_uses_in_operand(
    operand: &Operand<'_>,
    block: BlockId,
    stmt_ref: StatementRef,
    def_use: &mut HashMap<Local, LocalDefUse>,
) {
    walk_operand_locals(operand, &mut |local| {
        if let Some(du) = def_use.get_mut(&local) {
            du.uses.push(UseLocation {
                block,
                statement_ref: stmt_ref,
            });
        }
    });
}

/// Record a use of every local referenced by a place.
fn collect_uses_in_place(
    place: &Place,
    block: BlockId,
    stmt_ref: StatementRef,
    def_use: &mut HashMap<Local, LocalDefUse>,
) {
    walk_place_locals(place, &mut |local| {
        if let Some(du) = def_use.get_mut(&local) {
            du.uses.push(UseLocation {
                block,
                statement_ref: stmt_ref,
            });
        }
    });
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
                    statement_ref: StatementRef::Terminator,
                });
            }
        }
        Terminator::Branch { condition, .. } => {
            collect_uses_in_operand(condition, block, StatementRef::Terminator, def_use);
        }
        Terminator::NarrowBind {
            source,
            destination,
            ..
        } => {
            collect_uses_in_operand(source, block, StatementRef::Terminator, def_use);
            if let Some(du) = def_use.get_mut(destination) {
                du.def = Some(DefLocation {
                    block,
                    statement_ref: StatementRef::Terminator,
                    rvalue: Rvalue::Use(source.clone()),
                });
                du.all_defs.push((block, StatementRef::Terminator));
            }
        }
        Terminator::Switch { discriminant, .. } => {
            collect_uses_in_operand(discriminant, block, StatementRef::Terminator, def_use);
        }
        Terminator::Call {
            callee,
            args,
            runtime_id,
            destination,
            ..
        } => {
            collect_uses_in_operand(callee, block, StatementRef::Terminator, def_use);
            for arg in args {
                collect_uses_in_operand(arg, block, StatementRef::Terminator, def_use);
            }
            if let Some(runtime_id) = runtime_id {
                collect_uses_in_operand(runtime_id, block, StatementRef::Terminator, def_use);
            }
            // Record the def for the destination (where call result is stored)
            if let Place::Local(local) = destination {
                if let Some(du) = def_use.get_mut(local) {
                    // For Call terminators, we use a synthetic Rvalue::Use with a placeholder
                    // The actual value comes from the call, but for classification purposes,
                    // we just need to know there's a def here
                    du.def = Some(DefLocation {
                        block,
                        statement_ref: StatementRef::Terminator,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                    du.all_defs.push((block, StatementRef::Terminator));
                }
            }
        }
        Terminator::VirtualCall {
            args,
            runtime_id,
            destination,
            ..
        } => {
            // No callee operand — the method is resolved at runtime from `iface`.
            for arg in args {
                collect_uses_in_operand(arg, block, StatementRef::Terminator, def_use);
            }
            if let Some(runtime_id) = runtime_id {
                collect_uses_in_operand(runtime_id, block, StatementRef::Terminator, def_use);
            }
            // Record the def for the destination (where the call result is stored).
            if let Place::Local(local) = destination {
                if let Some(du) = def_use.get_mut(local) {
                    du.def = Some(DefLocation {
                        block,
                        statement_ref: StatementRef::Terminator,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                    du.all_defs.push((block, StatementRef::Terminator));
                }
            }
        }
        Terminator::SysOp {
            callee,
            args,
            runtime_id,
            destination,
            ..
        } => {
            collect_uses_in_operand(callee, block, StatementRef::Terminator, def_use);
            for arg in args {
                collect_uses_in_operand(arg, block, StatementRef::Terminator, def_use);
            }
            if let Some(runtime_id) = runtime_id {
                collect_uses_in_operand(runtime_id, block, StatementRef::Terminator, def_use);
            }
            // Record the def for the destination place
            if let Place::Local(local) = destination {
                if let Some(du) = def_use.get_mut(local) {
                    du.def = Some(DefLocation {
                        block,
                        statement_ref: StatementRef::Terminator,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                    du.all_defs.push((block, StatementRef::Terminator));
                }
            }
        }
        Terminator::Spawn {
            closure,
            name,
            config,
            future,
            ..
        } => {
            collect_uses_in_operand(closure, block, StatementRef::Terminator, def_use);
            collect_uses_in_operand(name, block, StatementRef::Terminator, def_use);
            if let Some(config) = config {
                collect_uses_in_operand(config, block, StatementRef::Terminator, def_use);
            }
            if let Place::Local(local) = future {
                if let Some(du) = def_use.get_mut(local) {
                    du.def = Some(DefLocation {
                        block,
                        statement_ref: StatementRef::Terminator,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                    du.all_defs.push((block, StatementRef::Terminator));
                }
            }
        }
        Terminator::Await {
            future,
            destination,
            ..
        } => {
            collect_uses_in_place(future, block, StatementRef::Terminator, def_use);
            // Record the def for the destination
            if let Place::Local(local) = destination {
                if let Some(du) = def_use.get_mut(local) {
                    du.def = Some(DefLocation {
                        block,
                        statement_ref: StatementRef::Terminator,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                    du.all_defs.push((block, StatementRef::Terminator));
                }
            }
        }
        Terminator::AwaitAny {
            futures,
            destination,
            ..
        } => {
            collect_uses_in_operand(futures, block, StatementRef::Terminator, def_use);
            // Record the def for the destination (the winning index)
            if let Place::Local(local) = destination {
                if let Some(du) = def_use.get_mut(local) {
                    du.def = Some(DefLocation {
                        block,
                        statement_ref: StatementRef::Terminator,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                    du.all_defs.push((block, StatementRef::Terminator));
                }
            }
        }
        Terminator::Throw { value }
        | Terminator::Rethrow { value }
        | Terminator::ThrowIfPanic { value, .. } => {
            collect_uses_in_operand(value, block, StatementRef::Terminator, def_use);
        }
        Terminator::ShortCircuit {
            operand,
            destination,
            ..
        } => {
            collect_uses_in_operand(operand, block, StatementRef::Terminator, def_use);
            // Record the def for the destination
            if let Place::Local(local) = destination {
                if let Some(du) = def_use.get_mut(local) {
                    du.def = Some(DefLocation {
                        block,
                        statement_ref: StatementRef::Terminator,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                    du.all_defs.push((block, StatementRef::Terminator));
                }
            }
        }
    }
}

// ============================================================================
// Local Classification
// ============================================================================

/// Everything classification reads, computed once per body.
struct ClassifyCx<'a, 'db> {
    body: &'a MirFunctionBody<'db>,
    arity: usize,
    def_use: &'a HashMap<Local, LocalDefUse<'db>>,
    dominators: &'a Dominators,
    /// Predecessors over the terminator edges.
    predecessors: &'a HashMap<BlockId, Vec<BlockId>>,
    trap_cfg: &'a TrapCfg,
    clobbers: &'a Clobbers,
    /// Whether reads collect frame type-arg slots. Only a body that rebinds a
    /// slot can have one clobbered, and walking templates is what makes read
    /// collection expensive, so every other body skips it.
    track_type_slots: bool,
    opt: OptLevel,
}

/// Whether any `BindType` rebinds a frame type-arg slot in this body.
fn body_rebinds_type_slots(body: &MirFunctionBody<'_>) -> bool {
    body.blocks.iter().any(|block| {
        block.statements.iter().any(|stmt| {
            matches!(
                stmt.kind,
                StatementKind::Intrinsic {
                    op: IntrinsicOp::BindType(_),
                    ..
                }
            )
        })
    })
}

/// What one block writes: on entry, per statement, and in its terminator.
struct BlockClobbers {
    /// What the VM writes when control enters the block (a handler's error
    /// bindings).
    entry: Resources,
    statements: Vec<Resources>,
    terminator: Resources,
    /// `entry`, every statement, and `terminator` together.
    all: Resources,
}

/// What every block writes, indexed by block id.
struct Clobbers {
    blocks: Vec<BlockClobbers>,
}

impl Clobbers {
    fn new(body: &MirFunctionBody<'_>) -> Self {
        let model = memory::ClobberModel::for_body(body);
        let blocks = body
            .blocks
            .iter()
            .map(|block| {
                let entry = memory::block_entry_clobbers(body, block.id);
                let statements: Vec<Resources> = block
                    .statements
                    .iter()
                    .map(|stmt| memory::statement_clobbers(body, model, &stmt.kind))
                    .collect();
                let terminator = block
                    .terminator
                    .as_ref()
                    .map(|terminator| memory::terminator_clobbers(model, terminator))
                    .unwrap_or_default();
                let mut all = entry.clone();
                for statement in &statements {
                    all.extend(statement);
                }
                all.extend(&terminator);
                BlockClobbers {
                    entry,
                    statements,
                    terminator,
                    all,
                }
            })
            .collect();
        Self { blocks }
    }

    fn block(&self, id: BlockId) -> &BlockClobbers {
        &self.blocks[id.0]
    }

    /// Whether a statement of `block` in `range` writes something in `reads`.
    fn statements_clobber(
        &self,
        block: BlockId,
        range: std::ops::Range<usize>,
        reads: &Resources,
    ) -> bool {
        self.block(block).statements[range]
            .iter()
            .any(|clobbers| clobbers.intersects(reads))
    }
}

/// Everything re-evaluating a definition's rvalue at a use reads: the rvalue's
/// own reads plus those of every single-definition local it reads, transitively.
///
/// A sunk evaluation re-evaluates the whole chain, and each link is judged
/// pairwise over its own segment: a link classified `Virtual` was proven
/// unclobbered from its definition to this one, and this one must be proven
/// unclobbered from here to the use for everything the chain reads. A local
/// with several definitions, or defined by a terminator, is a slot the sunk
/// evaluation loads, so it stays a leaf and the path walk finds its writes.
fn transitive_reads(cx: &ClassifyCx<'_, '_>, def: &DefLocation<'_>) -> Resources {
    let mut reads = memory::rvalue_reads(cx.body, &def.rvalue, cx.track_type_slots);
    let mut worklist: Vec<Local> = reads.locals.iter().copied().collect();
    let mut followed: HashSet<Local> = HashSet::new();
    while let Some(local) = worklist.pop() {
        if !followed.insert(local) {
            continue;
        }
        let du = &cx.def_use[&local];
        if du.all_defs.len() != 1 {
            continue;
        }
        let Some(inner) = &du.def else {
            continue;
        };
        if inner.statement_ref == StatementRef::Terminator {
            continue;
        }
        let inner_reads = memory::rvalue_reads(cx.body, &inner.rvalue, cx.track_type_slots);
        worklist.extend(inner_reads.locals.iter().copied());
        reads.extend(&inner_reads);
    }
    reads
}

/// Whether something in `reads` is written on some path from the definition at
/// statement `def_idx` of `def_block` to the use at `use_loc`.
///
/// Across blocks, the path's interior is exactly the blocks reachable from the
/// definition's successors and able to reach the use, both without entering
/// the definition's block — two reachability sweeps over the trap CFG, not path
/// enumeration. A path that re-enters the definition's block re-executes the
/// definition, so it is not a path the sunk evaluation observes. Interior
/// blocks are scanned whole, the use block included when a cycle brings the
/// use back to itself, so a write placed after the use that a later iteration
/// would observe counts too; the definition block's tail and terminator and the
/// use block's entry and head are scanned exactly.
fn clobbered_between(
    cx: &ClassifyCx<'_, '_>,
    def_block: BlockId,
    def_idx: usize,
    use_loc: &UseLocation,
    reads: &Resources,
) -> bool {
    let statement_count = |block: BlockId| cx.body.block(block).statements.len();
    let use_end = match use_loc.statement_ref {
        StatementRef::Statement(index) => index,
        StatementRef::Terminator => statement_count(use_loc.block),
    };
    if def_block == use_loc.block {
        return cx
            .clobbers
            .statements_clobber(def_block, def_idx + 1..use_end, reads);
    }
    if cx
        .clobbers
        .statements_clobber(def_block, def_idx + 1..statement_count(def_block), reads)
        || cx.clobbers.block(def_block).terminator.intersects(reads)
    {
        return true;
    }
    let forward = reachable_avoiding(
        cx.trap_cfg.successors(cx.body, def_block),
        def_block,
        |block| cx.trap_cfg.successors(cx.body, block),
    );
    let backward = reachable_avoiding(
        cx.trap_cfg.predecessors(use_loc.block),
        def_block,
        |block| cx.trap_cfg.predecessors(block),
    );
    if forward
        .intersection(&backward)
        .any(|block| cx.clobbers.block(*block).all.intersects(reads))
    {
        return true;
    }
    cx.clobbers.block(use_loc.block).entry.intersects(reads)
        || cx
            .clobbers
            .statements_clobber(use_loc.block, 0..use_end, reads)
}

/// How an rvalue's evaluation may be moved.
enum Repeatability {
    /// No reads and no effects: may be evaluated at any number of uses.
    Constant,
    /// May be evaluated at its use instead of its definition when nothing it
    /// reads is written in between. `once` marks an evaluation that is
    /// observable each time it runs — an allocation with observable identity,
    /// or an evaluation that can trap — which additionally may not run on a
    /// path that repeats the use without the definition.
    Movable { once: bool },
}

fn repeatability(body: &MirFunctionBody<'_>, rvalue: &Rvalue<'_>) -> Repeatability {
    if matches!(rvalue, Rvalue::Use(Operand::Constant(_))) {
        Repeatability::Constant
    } else {
        Repeatability::Movable {
            once: memory::rvalue_allocates_identity(rvalue)
                || memory::rvalue_can_trap(body, rvalue),
        }
    }
}

/// Whether a use in `block` is re-executed by a loop it heads. Sinking an
/// evaluation from outside a loop into its header repeats it every iteration:
/// a cost policy, not a soundness rule, since the path walk sees the body's
/// writes.
fn sinks_into_loop_header(cx: &ClassifyCx<'_, '_>, block: BlockId) -> bool {
    cx.predecessors
        .get(&block)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .any(|&pred| cx.dominators.dominates(block, pred))
}

/// Whether `use_block` can execute again without `def_block` executing first:
/// the shape `header -> body(use) -> header` for a definition before the loop.
fn use_repeats_without_definition(
    cx: &ClassifyCx<'_, '_>,
    def_block: BlockId,
    use_block: BlockId,
) -> bool {
    reachable_avoiding(
        cx.trap_cfg.successors(cx.body, use_block),
        def_block,
        |block| cx.trap_cfg.successors(cx.body, block),
    )
    .contains(&use_block)
}

/// Whether the local's single definition may be evaluated at its use instead
/// of where it is written, and the local dropped: the judgment behind
/// `Virtual`.
fn rematerializable(cx: &ClassifyCx<'_, '_>, du: &LocalDefUse<'_>) -> bool {
    let Some(def) = &du.def else {
        return false;
    };
    // The value comes from the operation itself, not from a re-emittable rvalue.
    if def.statement_ref == StatementRef::Terminator {
        return false;
    }
    // Several definitions would make "the" rvalue ambiguous.
    if du.all_defs.len() != 1 {
        return false;
    }
    let once = match repeatability(cx.body, &def.rvalue) {
        Repeatability::Constant => return !du.uses.is_empty(),
        Repeatability::Movable { once } => once,
    };
    // Evaluating a non-constant at several uses would repeat its work.
    if du.uses.len() != 1 {
        return false;
    }
    let use_loc = &du.uses[0];
    let StatementRef::Statement(def_idx) = def.statement_ref else {
        unreachable!("terminator definitions were rejected above");
    };
    if !cx.dominators.dominates(def.block, use_loc.block) {
        return false;
    }
    if def.block == use_loc.block
        && let StatementRef::Statement(use_idx) = use_loc.statement_ref
        && use_idx <= def_idx
    {
        return false;
    }
    let reads = transitive_reads(cx, def);
    if clobbered_between(cx, def.block, def_idx, use_loc, &reads) {
        return false;
    }
    if def.block != use_loc.block {
        // A trap is delivered to the handler of the block it happens in.
        if reads.order && !cx.trap_cfg.same_handler(def.block, use_loc.block) {
            return false;
        }
        if sinks_into_loop_header(cx, use_loc.block) {
            return false;
        }
        if once && use_repeats_without_definition(cx, def.block, use_loc.block) {
            return false;
        }
    }
    true
}

/// The parameter a local is a plain copy of, when every use may load the
/// parameter instead: `CopyOf`, the multi-use form of rematerialization for
/// the one rvalue whose re-evaluation is a single load.
///
/// Only a parameter qualifies as the source. Its slot exists before every use
/// whatever the source's own classification; another local's might not.
fn copy_source(cx: &ClassifyCx<'_, '_>, du: &LocalDefUse<'_>) -> Option<Local> {
    let def = du.def.as_ref()?;
    if def.statement_ref == StatementRef::Terminator || du.all_defs.len() != 1 {
        return None;
    }
    let source = match &def.rvalue {
        Rvalue::Use(Operand::Copy(Place::Local(source)) | Operand::Move(Place::Local(source))) => {
            *source
        }
        _ => return None,
    };
    if source.0 == 0 || source.0 > cx.arity {
        return None;
    }
    let StatementRef::Statement(def_idx) = def.statement_ref else {
        unreachable!("terminator definitions were rejected above");
    };
    let reads = memory::rvalue_reads(cx.body, &def.rvalue, cx.track_type_slots);
    du.uses
        .iter()
        .all(|use_loc| !clobbered_between(cx, def.block, def_idx, use_loc, &reads))
        .then_some(source)
}

/// Classify each local as Virtual, Real, `PhiLike`, `CopyOf`, or Dead.
///
/// Returns both the classifications and the `copy_sources` map for copy propagation.
fn classify_locals(
    cx: &ClassifyCx<'_, '_>,
    redirect_targets: &HashMap<BlockId, BlockId>,
) -> (HashMap<Local, LocalClassification>, HashMap<Local, Local>) {
    let ClassifyCx {
        body,
        arity,
        def_use,
        predecessors,
        opt,
        ..
    } = *cx;
    let mut classifications = HashMap::new();
    let mut copy_sources: HashMap<Local, Local> = HashMap::new();
    let mut stack_carry_candidates: HashMap<Local, stack_carry::StackCarryKind> = HashMap::new();
    let narrow_bind_destinations: HashSet<Local> = body
        .blocks
        .iter()
        .filter_map(|block| match block.terminator.as_ref() {
            Some(Terminator::NarrowBind { destination, .. }) => Some(*destination),
            _ => None,
        })
        .collect();

    for (idx, _local_decl) in body.locals.iter().enumerate() {
        let local = Local(idx);
        let du = &def_use[&local];

        let local_decl = body.local(local);

        // Check if this is an unused wildcard binding.
        // NOTE: We currently only check for exactly "_". In the future, we may want
        // more robust checking (e.g., any name starting with "_", or type-based analysis
        // to verify the binding truly has no observable side effects). For now, this
        // simple check handles the common pattern-matching wildcard case.
        let is_unused_wildcard = du.uses.is_empty() && local_decl.name.as_deref() == Some("_");

        // User-named locals (name.is_some()) are kept as Real at O0.
        // Compiler temps have name=None and are always eligible for optimization.
        let is_user_local = local_decl.name.is_some();

        let classification = if idx > 0 && idx <= arity {
            // Parameters are always real (they come from the caller)
            LocalClassification::Parameter
        } else if local_decl.is_captured {
            // Captured locals must always be Real - they need a stable stack slot
            // holding the cell (made by `FreshCell`, or the entry preamble for a
            // parameter) that `LoadDeref`/`StoreDeref` go through.
            // Virtual/CopyOf/PhiLike classification would inline away the slot.
            LocalClassification::Real
        } else if narrow_bind_destinations.contains(&local) {
            LocalClassification::Real
        } else if idx != 0
            && du.uses.is_empty()
            && (local_decl.name.is_none() || is_unused_wildcard)
        {
            // Dead local: either an unused compiler temp, or an unused wildcard binding.
            // Skip _0 which is implicitly used by return.
            LocalClassification::Dead
        } else if idx != 0
            && let Some(source) = copy_source(cx, du)
        {
            if opt == OptLevel::Zero && is_user_local {
                // At O0, keep user-named locals as Real.
                LocalClassification::Real
            } else {
                // Copy propagation: this local is just `_X = copy _Y` where _Y is suitable.
                // We can eliminate _X and use _Y directly at all use sites.
                copy_sources.insert(local, source);
                LocalClassification::CopyOf
            }
        } else if !(opt == OptLevel::Zero && is_user_local)
            && is_call_result_aggregate_operand(local, du, body, def_use)
        {
            stack_carry_candidates.insert(local, stack_carry::StackCarryKind::AggregateOperand);
            if rematerializable(cx, du) {
                LocalClassification::Virtual
            } else {
                LocalClassification::Real
            }
        } else if rematerializable(cx, du) {
            if opt == OptLevel::Zero && is_user_local {
                LocalClassification::Real
            } else {
                LocalClassification::Virtual
            }
        } else if is_stack_covered_phi(local, du, body, predecessors) {
            // Stack-carry candidate validated in a later stack simulation pass.
            stack_carry_candidates.insert(local, stack_carry::StackCarryKind::PhiLike);
            LocalClassification::Real
        } else if is_return_phi(local, body, def_use, redirect_targets) {
            // Stack-carry candidate validated in a later stack simulation pass.
            stack_carry_candidates.insert(local, stack_carry::StackCarryKind::ReturnPhi);
            LocalClassification::Real
        } else if is_call_result_immediate(local, du, body) {
            // Stack-carry candidate validated in a later stack simulation pass.
            stack_carry_candidates.insert(local, stack_carry::StackCarryKind::CallResultImmediate);
            LocalClassification::Real
        } else {
            LocalClassification::Real
        };

        classifications.insert(local, classification);
    }

    stack_carry::refine_stack_carry_classifications(
        body,
        def_use,
        &stack_carry_candidates,
        &mut classifications,
        predecessors,
    );

    (classifications, copy_sources)
}

/// Check if a local is "phi-like": every path into its single use leaves the
/// local's value on top of the operand stack, so the emitter can drop the
/// `StoreVar`/`LoadVar` pair and let the value ride the CFG edge instead.
///
/// This predicate is the *entire* soundness proof for `StackCarryKind::PhiLike`.
/// The stack simulation in [`crate::stack_carry`] starts AT the use block and
/// only validates that block's statements and the straight-line blocks Virtual
/// forwarding may carry the use into — it never inspects the local's
/// definitions, nor the use block's predecessors. So every def this
/// function accepts is emitted as a push with no store, and the use pops
/// exactly one value: an uncovered incoming edge leaves the pop consuming an
/// unrelated value, and a definition off the covered paths leaves a push that
/// nothing pops. Inside a loop the latter grows the operand stack every
/// iteration.
///
/// A local qualifies when all of:
///
/// 1. It has exactly one use, in block `U`.
/// 2. `U` is a join — at least two predecessors. Single-predecessor edges are a
///    different shape and deliberately out of scope here.
/// 3. Every predecessor covers the block it flows into, per
///    [`predecessors_cover_block`].
/// 4. Every definition of the local was recorded while proving (3); any other
///    definition is a stray push.
///
/// Checking only that a `ShortCircuit` terminator's `join` equals `U` is not a
/// substitute for (3): `merge_passthrough_blocks` in `baml_compiler2_mir`
/// rewrites a `ShortCircuit`'s `join` when the original join block is an empty
/// passthrough, and can retarget it onto a block that has unrelated incoming
/// edges. `let x = false; if (c) { x = a && b } x` ends up with the `if` join as
/// both the `ShortCircuit` join and the use block, while its other predecessor
/// is the `Branch` false edge, which pushes nothing.
fn is_stack_covered_phi(
    local: Local,
    du: &LocalDefUse,
    body: &MirFunctionBody<'_>,
    predecessors: &HashMap<BlockId, Vec<BlockId>>,
) -> bool {
    if du.uses.len() != 1 || du.all_defs.is_empty() {
        return false;
    }

    let use_block = du.uses[0].block;

    if predecessors
        .get(&use_block)
        .is_none_or(|preds| preds.len() < 2)
    {
        return false;
    }

    let mut visited = HashSet::new();
    let mut covered_defs = HashSet::new();
    if !predecessors_cover_block(
        local,
        use_block,
        body,
        predecessors,
        &mut visited,
        &mut covered_defs,
    ) {
        return false;
    }

    // No stray defs: everything that writes the local must be one of the pushes
    // the coverage walk accounted for.
    du.all_defs.iter().all(|def| covered_defs.contains(def))
}

/// Prove that control cannot reach `block` without the local's value on top of
/// the operand stack, recording the definitions that put it there.
///
/// Each predecessor must do one of:
///
/// a. End in `Goto { target: block }` with its last statement assigning the
///    local — under `PhiLike` the emitter emits the rvalue and skips the store,
///    so the value is left on the stack.
/// b. End in `ShortCircuit { destination: local, join: block, .. }` — the
///    `JumpIfFalse` peek leaves the LHS on the stack on the short-circuit edge.
/// c. Be a statement-free block ending in `Goto { target: block }` whose own
///    predecessors all cover it. The intermediate joins of a chain such as
///    `a && b && c` have this shape.
/// d. End in a call terminator that defines the local and returns into `block`
///    — see [`call_result_carried_into`]. `a || b.starts_with(c)` lowers the rhs
///    this way, so without this arm every short circuit over a call keeps its
///    destination in a slot.
///
/// `visited` rejects back-edges: a block reachable from itself would need a push
/// per trip to stay balanced, which this shape cannot prove.
///
/// Two kinds of block are refused outright, because the predecessor list is not
/// a complete account of how they are entered — see the guard below.
fn predecessors_cover_block(
    local: Local,
    block: BlockId,
    body: &MirFunctionBody<'_>,
    predecessors: &HashMap<BlockId, Vec<BlockId>>,
    visited: &mut HashSet<BlockId>,
    covered_defs: &mut HashSet<(BlockId, StatementRef)>,
) -> bool {
    if !visited.insert(block) {
        return false;
    }

    // `build_predecessors` is built purely from `Terminator::successors`, so it
    // only knows about entries that traverse a CFG edge. Two blocks are also
    // entered without one, and nothing is on the stack when they are:
    //
    // - `entry`, on every call of the function. A CFG predecessor set can look
    //   complete for it if the entry block is also a loop header.
    // - a catch region's handler, when the VM unwinds into it. A call's
    //   `unwind` target is a `successors` edge, but a same-frame panic — an
    //   overflowing `add_int`, a division by zero — reaches the handler from
    //   the middle of a block, with no edge at all. `compute_rpo` has to seed
    //   those handlers separately for exactly this reason, and `collect_def_use`
    //   records the VM's write of the error local there as an implicit use.
    //
    // Only the block being *covered* is refused. A handler that is itself a
    // predecessor stays fine: it reaches its successor over a real edge, having
    // pushed the value like any other predecessor.
    if block == body.entry
        || body
            .catch_regions
            .iter()
            .any(|region| region.handler == block)
    {
        return false;
    }

    let Some(preds) = predecessors.get(&block).filter(|preds| !preds.is_empty()) else {
        return false;
    };

    for &pred_id in preds {
        let pred = body.block(pred_id);

        if let Some(Terminator::ShortCircuit {
            destination: Place::Local(destination),
            join,
            ..
        }) = &pred.terminator
            && *destination == local
            && *join == block
        {
            covered_defs.insert((pred_id, StatementRef::Terminator));
            continue;
        }

        if call_result_carried_into(pred.terminator.as_ref(), block) == Some(local) {
            covered_defs.insert((pred_id, StatementRef::Terminator));
            continue;
        }

        let goes_to_block = matches!(
            &pred.terminator,
            Some(Terminator::Goto { target }) if *target == block
        );
        if !goes_to_block {
            return false;
        }

        let Some(last) = pred.statements.last() else {
            // Empty passthrough — push the proof up to its own predecessors.
            if !predecessors_cover_block(local, pred_id, body, predecessors, visited, covered_defs)
            {
                return false;
            }
            continue;
        };

        let assigns_local = matches!(
            &last.kind,
            StatementKind::Assign { destination: Place::Local(l), .. } if *l == local
        );
        if !assigns_local {
            return false;
        }

        covered_defs.insert((pred_id, StatementRef::Statement(pred.statements.len() - 1)));
    }

    true
}

/// The local a call terminator leaves on top of the operand stack in `block`,
/// when the call's result is stack-carried rather than stored.
///
/// `Call` and `VirtualCall` both emit as `<call opcode>`, then
/// `emit_store_place(destination)` — a no-op for a stack-carried destination —
/// then the jump to `target`. So control arrives at `target` with the call's one
/// result on top, exactly like a predecessor that assigns and falls through.
///
/// The other call-shaped terminators are deliberately not here:
///
/// - `Await` and `AwaitAny` suspend the engine, and the note on
///   `is_call_result_immediate` records that their opcodes rewind and
///   re-execute across that suspend.
/// - `SysOp` also suspends, and its dispatch asserts that the arguments are the
///   whole of the frame's operand stack.
/// - `Spawn` defines the spawned future rather than a call result, and
///   continues at `resume`.
///
/// None of those covers a join anywhere in the corpus, so allowing them would be
/// speculation instead of a proof.
///
/// `is_call_result_immediate` also excludes `VirtualCall`, but for a reason that
/// does not apply here: that exclusion exists because the `CallResultImmediate`
/// stack simulation has to recognize the *defining* terminator to know which
/// block to start simulating from, and its match has no `VirtualCall` arm. The
/// `PhiLike` simulation starts at the use block and never looks at the def.
fn call_result_carried_into(terminator: Option<&Terminator<'_>>, block: BlockId) -> Option<Local> {
    let (Terminator::Call {
        destination,
        target,
        unwind,
        ..
    }
    | Terminator::VirtualCall {
        destination,
        target,
        unwind,
        ..
    }) = terminator?
    else {
        return None;
    };

    // A call that unwinds into `block` also reaches it on the throwing edge,
    // where nothing was pushed. `Terminator::successors` reports both edges, so
    // without this the same predecessor would be counted as covering twice.
    if *target != block || *unwind == Some(block) {
        return None;
    }

    match destination {
        Place::Local(local) => Some(*local),
        _ => None,
    }
}

/// Check if a MIR statement is stack-neutral (doesn't push or pop from the eval stack).
///
/// Stack-neutral statements can safely execute while a value meant for return sits on
/// the stack, enabling optimizations like `ReturnPhi` even when there are statements
/// between the assignment to `_0` and the `Return` terminator.
fn is_stack_neutral_statement(kind: &StatementKind<'_>) -> bool {
    match kind {
        // Replaces a captured cell in place - doesn't touch the stack
        StatementKind::FreshCell { .. } => true,
        // Intrinsics push args then SendEvent consumes them - net neutral
        StatementKind::Intrinsic { .. } => true,
        StatementKind::Nop => true,

        // These modify the stack
        StatementKind::Assign { .. } => false,
        StatementKind::Drop(_) => false,
        // Pushes receiver, value and the interface type, then the opcode pops all
        // three — net neutral, but it touches the stack in between, so a value
        // parked there for `Return` would be buried.
        StatementKind::VirtualFieldStore { .. } => false,
    }
}

/// Check if `_0` (the return place) is a "return-phi" local.
///
/// Return-phi applies when `_0` is assigned before Return in each defining block,
/// with only stack-neutral statements (like `FreshCell`) between the assignment
/// and Return. This allows us to:
/// - At def sites: emit rvalue but NOT `StoreVar` (leave value on stack)
/// - At Return: skip `LoadVar` for _0 (value already on stack)
///
/// This eliminates the redundant `StoreVar("_0"); LoadVar("_0"); Return` pattern.
fn is_return_phi(
    local: Local,
    body: &MirFunctionBody<'_>,
    def_use: &HashMap<Local, LocalDefUse>,
    redirect_targets: &HashMap<BlockId, BlockId>,
) -> bool {
    // Only applies to _0 (the return place)
    if local.0 != 0 {
        return false;
    }

    // Get all definitions of _0
    let defs = &def_use[&local].all_defs;

    // Must have at least one definition
    if defs.is_empty() {
        return false;
    }

    // Helper: check if a block leads to Return through only stack-neutral statements.
    // Follows Goto chains, ensuring all intermediate blocks have only stack-neutral statements.
    let leads_to_return_safely = |start: BlockId| -> bool {
        let mut current = start;
        let mut visited = HashSet::new();

        loop {
            // Avoid infinite loops
            if !visited.insert(current) {
                return false;
            }

            let block = body.block(current);

            // All statements in this block must be stack-neutral
            if !block
                .statements
                .iter()
                .all(|s| is_stack_neutral_statement(&s.kind))
            {
                return false;
            }

            match &block.terminator {
                Some(Terminator::Return) => return true,
                Some(Terminator::Goto { target }) => {
                    // Follow the redirect chain
                    current = redirect_targets.get(target).copied().unwrap_or(*target);
                }
                _ => return false,
            }
        }
    };

    // Each definition block must:
    // 1. Have the definition followed only by stack-neutral statements (or be a terminator definition)
    // 2. End with Return OR lead to Return through only stack-neutral blocks
    for &(block_id, stmt_ref) in defs {
        let block = body.block(block_id);

        let stmt_idx = match stmt_ref {
            StatementRef::Terminator => {
                // For terminator definitions, check if the continuation leads to return safely
                let continuation = match &block.terminator {
                    Some(Terminator::Call { target, .. }) => Some(*target),
                    Some(Terminator::SysOp { target, .. }) => Some(*target),
                    Some(Terminator::Await { target, .. }) => Some(*target),
                    Some(Terminator::AwaitAny { target, .. }) => Some(*target),
                    _ => None,
                };
                let valid = continuation.is_some_and(leads_to_return_safely);
                if !valid {
                    return false;
                }
                continue;
            }
            StatementRef::Statement(idx) => idx,
        };

        // For regular Assign statements: all statements after the definition must be stack-neutral
        let statements_after_def_are_neutral = block.statements[stmt_idx + 1..]
            .iter()
            .all(|s| is_stack_neutral_statement(&s.kind));
        if !statements_after_def_are_neutral {
            return false;
        }

        // Block must end with Return or lead to Return through stack-neutral blocks
        let valid_terminator = match &block.terminator {
            Some(Terminator::Return) => true,
            Some(Terminator::Goto { target }) => {
                let resolved = redirect_targets.get(target).copied().unwrap_or(*target);
                leads_to_return_safely(resolved)
            }
            _ => false,
        };

        if !valid_terminator {
            return false;
        }
    }

    true
}

fn successors_of(body: &MirFunctionBody<'_>, block: BlockId) -> Vec<BlockId> {
    body.block(block)
        .terminator
        .as_ref()
        .map_or_else(Vec::new, Terminator::successors)
}

/// Every block reachable from `start` by repeatedly following `next`, never
/// entering `barrier` (so never yielding it either).
fn reachable_avoiding(
    start: Vec<BlockId>,
    barrier: BlockId,
    next: impl Fn(BlockId) -> Vec<BlockId>,
) -> HashSet<BlockId> {
    let mut worklist = start;
    let mut visited = HashSet::new();
    while let Some(block) = worklist.pop() {
        if block == barrier || !visited.insert(block) {
            continue;
        }
        worklist.extend(next(block));
    }
    visited
}

/// Check if a local is a "call result immediate": defined by a call-like terminator,
/// used exactly once in the continuation block.
///
/// Call result immediate applies when:
/// 1. The local has exactly one definition, from Call/VirtualCall/Await/SysOp
/// 2. It has exactly one use
/// 3. The use is in the continuation block (target of the Call)
///
/// This allows us to:
/// - After Call: don't emit `StoreVar` (leave result on stack)
/// - At use site: don't emit `LoadVar` (value already on stack from Call)
///
/// This eliminates the redundant `StoreVar("_X"); LoadVar("_X")` pattern for call results.
fn is_call_result_immediate(local: Local, du: &LocalDefUse, body: &MirFunctionBody<'_>) -> bool {
    // Must have exactly one use
    if du.uses.len() != 1 || du.all_defs.len() != 1 {
        return false;
    }

    // A class spread is emitted incrementally as
    // `AllocInstance; InitField/InitSpread`. Its explicit field operands must
    // be pushed after the destination instance exists. Carrying a call result
    // from the preceding block leaves it below that instance and reverses the
    // `InitField` operands. Reject this structurally here, including when the
    // aggregate destination is virtual and its use is forwarded elsewhere.
    let use_loc = &du.uses[0];
    if let StatementRef::Statement(stmt_idx) = use_loc.statement_ref
        && let Some(StatementKind::Assign {
            value:
                Rvalue::Aggregate {
                    kind: baml_compiler2_mir::AggregateKind::Class { .. },
                    fields,
                },
            ..
        }) = body
            .block(use_loc.block)
            .statements
            .get(stmt_idx)
            .map(|stmt| &stmt.kind)
        && fields.iter().any(is_class_field_copy_operand)
    {
        return false;
    }

    // Must have a definition from a call-like terminator.
    let Some(def) = &du.def else {
        return false;
    };

    // Definition must be in a terminator
    if def.statement_ref != StatementRef::Terminator {
        return false;
    }

    // Get the defining block and check that its terminator is call-like
    // that defines this local.
    let def_block = body.block(def.block);
    match &def_block.terminator {
        Some(
            Terminator::Call { destination, .. } | Terminator::VirtualCall { destination, .. },
        ) => {
            matches!(destination, Place::Local(l) if *l == local)
        }
        Some(Terminator::Await { destination, .. }) => {
            matches!(destination, Place::Local(l) if *l == local)
        }
        // NOTE: `AwaitAny` is intentionally NOT treated as a call-result
        // immediate. Its opcode rewinds + re-executes across the engine
        // suspend (like `Await`), but its result also commonly feeds straight
        // into an indexed `await futures[i]`; carrying the result on the stack
        // across that combination misaligns the stack. Always store it to a
        // local instead (correct, marginally less optimal).
        Some(Terminator::SysOp { destination, .. }) => {
            matches!(destination, Place::Local(l) if *l == local)
        }
        _ => false,
    }
}

/// Check if a call-like result is used as part of a stack-consumable aggregate prefix.
///
/// Map and array allocation consume values in source order, so a chain like
/// `v1 = call ...; v2 = call ...; map { "a": v1, "b": v2 }` can keep `v1`
/// and `v2` on the VM stack until the final `alloc_map`.
fn is_call_result_aggregate_operand(
    local: Local,
    du: &LocalDefUse,
    body: &MirFunctionBody<'_>,
    def_use: &HashMap<Local, LocalDefUse>,
) -> bool {
    if !is_call_like_result_local(local, du, body) {
        return false;
    }

    let [use_loc] = du.uses.as_slice() else {
        return false;
    };
    let Some(operands) = stack_prefix_operands(body, use_loc) else {
        return false;
    };

    let mut found_local = false;
    for operand in operands {
        let Some(operand_local) = operand_local(operand) else {
            return false;
        };

        if operand_local == local {
            found_local = true;
            break;
        }

        let Some(operand_du) = def_use.get(&operand_local) else {
            return false;
        };
        if !is_call_like_result_local(operand_local, operand_du, body) {
            return false;
        }
        let [operand_use] = operand_du.uses.as_slice() else {
            return false;
        };
        if operand_use.block != use_loc.block || operand_use.statement_ref != use_loc.statement_ref
        {
            return false;
        }
    }

    found_local
}

pub(crate) fn stack_prefix_operands<'a, 'db>(
    body: &'a MirFunctionBody<'db>,
    use_loc: &UseLocation,
) -> Option<Vec<&'a Operand<'db>>> {
    let block = body.block(use_loc.block);
    match use_loc.statement_ref {
        StatementRef::Statement(index) => match &block.statements.get(index)?.kind {
            StatementKind::Assign { value, .. } => aggregate_stack_prefix_operands(value),
            _ => None,
        },
        StatementRef::Terminator => match block.terminator.as_ref()? {
            Terminator::Call { args, .. } | Terminator::VirtualCall { args, .. } => {
                Some(args.iter().collect())
            }
            _ => None,
        },
    }
}

fn aggregate_stack_prefix_operands<'a, 'db>(
    rvalue: &'a Rvalue<'db>,
) -> Option<Vec<&'a Operand<'db>>> {
    match rvalue {
        Rvalue::BinaryOp { left, right, .. } => Some(vec![left, right]),
        Rvalue::Array(_, elements) => Some(elements.iter().collect()),
        // Map lowering emits all values first, then all keys, because the VM
        // consumes maps as `[v1, v2, ..., k1, k2, ...]`. A carried key would sit
        // below the emitted values, so only value positions are stack-carryable.
        Rvalue::Map(_, _, entries) => Some(entries.iter().map(|(_key, value)| value).collect()),
        Rvalue::Aggregate {
            kind: baml_compiler2_mir::AggregateKind::Array,
            fields,
        } => Some(fields.iter().collect()),
        Rvalue::Aggregate {
            kind: baml_compiler2_mir::AggregateKind::Class { .. },
            fields,
        } if !fields.iter().any(is_class_field_copy_operand) => Some(fields.iter().collect()),
        // Class aggregates with field-copy operands use the `init_spread` path
        // instead of the field-value init plan, so stack-carried values would
        // not be consumed in the order modeled here.
        Rvalue::Aggregate { .. } => None,
        _ => None,
    }
}

fn is_class_field_copy_operand(operand: &Operand<'_>) -> bool {
    let place = match operand {
        Operand::Copy(place) | Operand::Move(place) => place,
        Operand::Constant(_) => return false,
    };
    matches!(place, Place::Field { .. })
}

fn operand_local(operand: &Operand<'_>) -> Option<Local> {
    match operand {
        Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => Some(*local),
        _ => None,
    }
}

fn is_call_like_result_local(local: Local, du: &LocalDefUse, body: &MirFunctionBody<'_>) -> bool {
    if du.uses.len() != 1 || du.all_defs.len() != 1 {
        return false;
    }

    let Some(def) = &du.def else {
        return false;
    };
    if def.statement_ref != StatementRef::Terminator {
        return def.block != du.uses[0].block
            && matches!(
                def.rvalue,
                Rvalue::Use(Operand::Constant(
                    Constant::Int(_)
                        | Constant::Bool(_)
                        | Constant::Float(_)
                        | Constant::String(_)
                        | Constant::Null
                ))
            );
    }

    let def_block = body.block(def.block);
    match &def_block.terminator {
        Some(
            Terminator::Call { destination, .. }
            | Terminator::VirtualCall { destination, .. }
            | Terminator::Await { destination, .. }
            // `AwaitAny` deliberately excluded — see the note in the sibling
            // call-result-immediate check above.
            | Terminator::SysOp { destination, .. },
        ) => {
            matches!(destination, Place::Local(l) if *l == local)
        }
        _ => false,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use baml_compiler2_mir::{
        BasicBlock, CatchRegion, CellId, Constant, LocalDecl, MirFunctionBody, Operand, Place,
        Statement, Terminator,
    };
    use baml_type::RuntimeTy;

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

    #[test]
    fn aggregate_operand_allows_non_carried_trailing_operands() {
        let target = Local(1);
        let body = MirFunctionBody {
            blocks: vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: vec![],
                    terminator: Some(Terminator::Call {
                        argument_layout: None,
                        callee: Operand::Constant(Constant::Null),
                        args: vec![],
                        ntypeargs: 0,
                        runtime_id: None,
                        destination: Place::Local(target),
                        target: BlockId(1),
                        unwind: None,
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![Statement {
                        kind: StatementKind::Assign {
                            destination: Place::Local(Local(0)),
                            value: Rvalue::Array(
                                baml_type::TyTemplate::from(baml_type::RealizedTy::unknown()),
                                vec![
                                    Operand::copy_local(target),
                                    Operand::Constant(Constant::Int(1)),
                                ],
                            ),
                        },
                        span: None,
                    }],
                    terminator: Some(Terminator::Return),
                    span: None,
                    terminator_span: None,
                },
            ],
            entry: BlockId(0),
            locals: vec![],
            catch_regions: vec![],
        };
        let du = LocalDefUse {
            def: Some(DefLocation {
                block: BlockId(0),
                statement_ref: StatementRef::Terminator,
                rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
            }),
            uses: vec![UseLocation {
                block: BlockId(1),
                statement_ref: StatementRef::Statement(0),
            }],
            all_defs: vec![(BlockId(0), StatementRef::Terminator)],
        };
        let def_use = HashMap::from([(target, du.clone())]);

        assert!(is_call_result_aggregate_operand(
            target, &du, &body, &def_use,
        ));
    }

    #[test]
    fn call_result_immediate_rejects_incremental_class_spread_init() {
        let result = Local(1);
        let spread_base = Local(2);
        let body = MirFunctionBody {
            blocks: vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: vec![],
                    terminator: Some(Terminator::Call {
                        argument_layout: None,
                        callee: Operand::Constant(Constant::Null),
                        args: vec![],
                        ntypeargs: 0,
                        runtime_id: None,
                        destination: Place::Local(result),
                        target: BlockId(1),
                        unwind: None,
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![Statement {
                        kind: StatementKind::Assign {
                            destination: Place::Local(Local(0)),
                            value: Rvalue::Aggregate {
                                kind: baml_compiler2_mir::AggregateKind::Class {
                                    name: "GuideHooks".to_string(),
                                    type_arg_templates: vec![],
                                },
                                fields: vec![
                                    Operand::copy_local(result),
                                    Operand::Copy(Place::Field {
                                        base: Box::new(Place::Local(spread_base)),
                                        field: 1,
                                    }),
                                ],
                            },
                        },
                        span: None,
                    }],
                    terminator: Some(Terminator::Return),
                    span: None,
                    terminator_span: None,
                },
            ],
            entry: BlockId(0),
            locals: vec![],
            catch_regions: vec![],
        };
        let du = LocalDefUse {
            def: Some(DefLocation {
                block: BlockId(0),
                statement_ref: StatementRef::Terminator,
                rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
            }),
            uses: vec![UseLocation {
                block: BlockId(1),
                statement_ref: StatementRef::Statement(0),
            }],
            all_defs: vec![(BlockId(0), StatementRef::Terminator)],
        };

        assert!(!is_call_result_immediate(result, &du, &body));
    }

    /// Builds a minimal integer local declaration for MIR analysis tests.
    fn int_local_decl(name: Option<&str>) -> LocalDecl {
        LocalDecl {
            name: name.map(baml_base::Name::new),
            ty: RuntimeTy::Int,
            span: None,
            scope_span: None,
            is_captured: false,
        }
    }

    fn int_list_local_decl(name: Option<&str>) -> LocalDecl {
        LocalDecl {
            name: name.map(baml_base::Name::new),
            ty: RuntimeTy::list(RuntimeTy::int()),
            span: None,
            scope_span: None,
            is_captured: false,
        }
    }

    /// `a && b && c`: two chained `ShortCircuit` terminators whose inner join
    /// (bb3) is an empty passthrough into the outer join (bb4).
    fn nested_short_circuit_body(
        name: Option<&str>,
        with_prior_definition: bool,
    ) -> MirFunctionBody<'static> {
        let destination = Local(1);
        let prior_definition = with_prior_definition.then(|| assign_bool(destination, false));

        bool_body(
            vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: prior_definition.into_iter().collect(),
                    terminator: Some(Terminator::ShortCircuit {
                        operand: Operand::Constant(Constant::Bool(true)),
                        kind: baml_compiler2_mir::ShortCircuitKind::And,
                        destination: Place::Local(destination),
                        eval_rhs: BlockId(1),
                        join: BlockId(4),
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![],
                    terminator: Some(Terminator::ShortCircuit {
                        operand: Operand::Constant(Constant::Bool(true)),
                        kind: baml_compiler2_mir::ShortCircuitKind::And,
                        destination: Place::Local(destination),
                        eval_rhs: BlockId(2),
                        join: BlockId(3),
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(2),
                    statements: vec![assign_bool(destination, true)],
                    terminator: Some(Terminator::Goto { target: BlockId(3) }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(3),
                    statements: vec![],
                    terminator: Some(Terminator::Goto { target: BlockId(4) }),
                    span: None,
                    terminator_span: None,
                },
                return_local_block(BlockId(4), destination),
            ],
            name,
        )
    }

    fn bool_local_decl(name: Option<&str>) -> LocalDecl {
        LocalDecl {
            name: name.map(baml_base::Name::new),
            ty: RuntimeTy::Bool,
            span: None,
            scope_span: None,
            is_captured: false,
        }
    }

    fn assign_bool(destination: Local, value: bool) -> Statement<'static> {
        Statement {
            kind: StatementKind::Assign {
                destination: Place::Local(destination),
                value: Rvalue::Use(Operand::Constant(Constant::Bool(value))),
            },
            span: None,
        }
    }

    /// `_0 = copy destination; return` — the single use of the carried local.
    fn return_local_block(id: BlockId, destination: Local) -> BasicBlock<'static> {
        BasicBlock {
            id,
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    destination: Place::Local(Local(0)),
                    value: Rvalue::Use(Operand::copy_local(destination)),
                },
                span: None,
            }],
            terminator: Some(Terminator::Return),
            span: None,
            terminator_span: None,
        }
    }

    fn bool_body(blocks: Vec<BasicBlock<'static>>, name: Option<&str>) -> MirFunctionBody<'static> {
        MirFunctionBody {
            blocks,
            entry: BlockId(0),
            locals: vec![bool_local_decl(None), bool_local_decl(name)],
            catch_regions: vec![],
        }
    }

    fn is_stack_covered(body: &MirFunctionBody<'_>, local: Local) -> bool {
        let def_use = collect_def_use(body);
        let predecessors = build_predecessors(body);

        is_stack_covered_phi(local, &def_use[&local], body, &predecessors)
    }

    fn analyzed_classification(body: &MirFunctionBody<'_>, local: Local) -> LocalClassification {
        AnalysisResult::analyze(body, 0, OptLevel::One).classifications[&local]
    }

    fn block(
        id: usize,
        statements: Vec<Statement<'static>>,
        terminator: Terminator<'static>,
    ) -> BasicBlock<'static> {
        BasicBlock {
            id: BlockId(id),
            statements,
            terminator: Some(terminator),
            span: None,
            terminator_span: None,
        }
    }

    fn goto(target: usize) -> Terminator<'static> {
        Terminator::Goto {
            target: BlockId(target),
        }
    }

    fn branch(then_block: usize, else_block: usize) -> Terminator<'static> {
        Terminator::Branch {
            condition: Operand::Constant(Constant::Bool(true)),
            then_block: BlockId(then_block),
            else_block: BlockId(else_block),
        }
    }

    /// `type T = …` on frame slot `slot`; the operand is immaterial here.
    fn bind_type(slot: u32) -> Statement<'static> {
        Statement {
            kind: StatementKind::Intrinsic {
                op: IntrinsicOp::BindType(slot),
                args: vec![Operand::Constant(Constant::Null)],
            },
            span: None,
        }
    }

    /// `reflect.Type.of<T>()` for the `T` bound on frame slot `slot`.
    fn load_type_slot(destination: Local, slot: u32) -> Statement<'static> {
        Statement {
            kind: StatementKind::Assign {
                destination: Place::Local(destination),
                value: Rvalue::LoadType(baml_type::TyTemplate::TypeArgRef(slot)),
            },
            span: None,
        }
    }

    fn copy_into(destination: Local, source: Local) -> Statement<'static> {
        Statement {
            kind: StatementKind::Assign {
                destination: Place::Local(destination),
                value: Rvalue::Use(Operand::copy_local(source)),
            },
            span: None,
        }
    }

    /// `destination = *cell`: read the value behind a captured local's cell.
    fn copy_cell_into(destination: Local, cell: Local) -> Statement<'static> {
        Statement {
            kind: StatementKind::Assign {
                destination: Place::Local(destination),
                value: Rvalue::Use(Operand::Copy(Place::Deref(CellId::Local(cell)))),
            },
            span: None,
        }
    }

    /// Two sequential binding blocks reuse one frame slot; the descriptor
    /// computed under the first is consumed after the second has rebound it.
    fn sibling_rebinding_body(rebinds_between: bool) -> MirFunctionBody<'static> {
        let between = if rebinds_between {
            vec![bind_type(0)]
        } else {
            vec![]
        };
        MirFunctionBody {
            blocks: vec![
                block(0, vec![bind_type(0), load_type_slot(Local(1), 0)], goto(1)),
                block(1, between, goto(2)),
                block(2, vec![copy_into(Local(0), Local(1))], Terminator::Return),
            ],
            entry: BlockId(0),
            locals: vec![int_local_decl(None), int_local_decl(None)],
            catch_regions: vec![],
        }
    }

    #[test]
    fn a_sibling_rebinding_of_a_read_slot_keeps_the_descriptor_materialized() {
        let body = sibling_rebinding_body(true);
        assert_eq!(
            analyzed_classification(&body, Local(1)),
            LocalClassification::Real
        );
    }

    #[test]
    fn a_slot_bound_once_lets_its_descriptor_sink_across_blocks() {
        let body = sibling_rebinding_body(false);
        assert_eq!(
            analyzed_classification(&body, Local(1)),
            LocalClassification::Virtual
        );
    }

    /// A loop whose body rebinds the slot at its scope top and recomputes the
    /// descriptor each iteration: the only path from the definition back to
    /// the rebinding re-executes the definition, so sinking stays sound.
    #[test]
    fn a_rebinding_reached_only_through_the_definition_does_not_block_sinking() {
        let body = MirFunctionBody {
            blocks: vec![
                block(0, vec![], goto(1)),
                block(1, vec![], branch(2, 4)),
                block(2, vec![bind_type(0), load_type_slot(Local(1), 0)], goto(3)),
                block(3, vec![copy_into(Local(2), Local(1))], goto(1)),
                block(4, vec![], Terminator::Return),
            ],
            entry: BlockId(0),
            locals: vec![
                int_local_decl(None),
                int_local_decl(None),
                int_local_decl(None),
            ],
            catch_regions: vec![],
        };
        assert_eq!(
            analyzed_classification(&body, Local(1)),
            LocalClassification::Virtual
        );
    }

    /// The descriptor is computed before a loop and consumed inside it, and the
    /// loop body rebinds the slot after the use: the second iteration's use
    /// would observe the rebinding, so the descriptor must be materialized.
    #[test]
    fn a_rebinding_after_the_use_inside_a_cycle_keeps_the_descriptor_materialized() {
        let body = MirFunctionBody {
            blocks: vec![
                block(0, vec![bind_type(0), load_type_slot(Local(1), 0)], goto(1)),
                block(1, vec![], branch(2, 3)),
                block(
                    2,
                    vec![copy_into(Local(2), Local(1)), bind_type(0)],
                    goto(1),
                ),
                block(3, vec![], Terminator::Return),
            ],
            entry: BlockId(0),
            locals: vec![
                int_local_decl(None),
                int_local_decl(None),
                int_local_decl(None),
            ],
            catch_regions: vec![],
        };
        assert_eq!(
            analyzed_classification(&body, Local(1)),
            LocalClassification::Real
        );
    }

    /// A rebinding that only a trap can reach: the protected block has no
    /// call, so its handler has no terminator edge, but a division inside it
    /// lands there through the exception table and the use runs afterwards.
    #[test]
    fn a_rebinding_in_a_trap_handler_keeps_the_descriptor_materialized() {
        let body = MirFunctionBody {
            blocks: vec![
                block(0, vec![bind_type(0), load_type_slot(Local(1), 0)], goto(1)),
                block(1, vec![], goto(2)),
                block(2, vec![copy_into(Local(0), Local(1))], Terminator::Return),
                block(3, vec![bind_type(0)], goto(2)),
            ],
            entry: BlockId(0),
            locals: vec![
                int_local_decl(None),
                int_local_decl(None),
                int_local_decl(None),
            ],
            catch_regions: vec![CatchRegion {
                body_entry: BlockId(1),
                handler: BlockId(3),
                body_blocks: vec![BlockId(1)],
                handler_body: vec![BlockId(3)],
                error_local: Local(2),
                stack_trace_local: None,
            }],
        };
        assert_eq!(
            analyzed_classification(&body, Local(1)),
            LocalClassification::Real
        );
    }

    /// A captured integer local: its slot holds a cell that closures (and
    /// other tasks) write to.
    fn captured_int_local_decl(name: Option<&str>) -> LocalDecl {
        LocalDecl {
            is_captured: true,
            ..int_local_decl(name)
        }
    }

    fn call_into_dest(destination: Local, target: usize) -> Terminator<'static> {
        Terminator::Call {
            argument_layout: None,
            callee: Operand::Constant(Constant::Null),
            args: vec![],
            ntypeargs: 0,
            runtime_id: None,
            destination: Place::Local(destination),
            target: BlockId(target),
            unwind: None,
        }
    }

    fn assign_int(destination: Place, value: i64) -> Statement<'static> {
        Statement {
            kind: StatementKind::Assign {
                destination,
                value: Rvalue::Use(Operand::Constant(Constant::Int(value))),
            },
            span: None,
        }
    }

    /// `_1 = x` snapshotted in one block and consumed in the next, with `x` a
    /// cell (`is_captured`); the terminator between them is the parameter.
    fn captured_read_across_blocks(between: Terminator<'static>) -> MirFunctionBody<'static> {
        MirFunctionBody {
            blocks: vec![
                block(0, vec![copy_cell_into(Local(1), Local(2))], between),
                block(1, vec![copy_into(Local(0), Local(1))], Terminator::Return),
            ],
            entry: BlockId(0),
            locals: vec![
                int_local_decl(None),
                int_local_decl(None),
                captured_int_local_decl(Some("x")),
                int_local_decl(None),
            ],
            catch_regions: vec![],
        }
    }

    /// The call may run a closure that writes the cell; re-reading `x` at the
    /// use would observe it.
    #[test]
    fn a_captured_read_never_sinks_across_a_call() {
        let body = captured_read_across_blocks(call_into_dest(Local(3), 1));
        assert_eq!(
            analyzed_classification(&body, Local(1)),
            LocalClassification::Real
        );
    }

    /// With nothing but a jump between the blocks nothing can write the cell,
    /// so the read sinks: the rule is about writers on the path, not blocks.
    #[test]
    fn a_captured_read_sinks_across_a_bare_jump() {
        let body = captured_read_across_blocks(goto(1));
        assert_eq!(
            analyzed_classification(&body, Local(1)),
            LocalClassification::Virtual
        );
    }

    /// The same read of a non-captured local sinks as before.
    #[test]
    fn a_plain_read_still_sinks_across_a_block_boundary() {
        let body = MirFunctionBody {
            blocks: vec![
                block(
                    0,
                    vec![
                        assign_int(Place::Local(Local(2)), 7),
                        copy_into(Local(1), Local(2)),
                    ],
                    goto(1),
                ),
                block(1, vec![copy_into(Local(0), Local(1))], Terminator::Return),
            ],
            entry: BlockId(0),
            locals: vec![
                int_local_decl(None),
                int_local_decl(None),
                int_local_decl(None),
            ],
            catch_regions: vec![],
        };
        assert_eq!(
            analyzed_classification(&body, Local(1)),
            LocalClassification::Virtual
        );
    }

    /// Inside a closure: the read goes through a capture slot, and the next
    /// block writes that slot before the use.
    #[test]
    fn a_capture_slot_read_never_sinks_across_a_block_boundary() {
        let read_capture = Statement {
            kind: StatementKind::Assign {
                destination: Place::Local(Local(1)),
                value: Rvalue::Use(Operand::Copy(Place::Deref(CellId::Capture(0)))),
            },
            span: None,
        };
        let body = MirFunctionBody {
            blocks: vec![
                block(0, vec![read_capture], goto(1)),
                block(
                    1,
                    vec![
                        assign_int(Place::Deref(CellId::Capture(0)), 5),
                        copy_into(Local(0), Local(1)),
                    ],
                    Terminator::Return,
                ),
            ],
            entry: BlockId(0),
            locals: vec![int_local_decl(None), int_local_decl(None)],
            catch_regions: vec![],
        };
        assert_eq!(
            analyzed_classification(&body, Local(1)),
            LocalClassification::Real
        );
    }

    /// A copy of a never-reassigned parameter is normally read straight from
    /// the parameter at its uses; a captured parameter is a cell, so it is not.
    #[test]
    fn a_captured_parameter_is_never_a_copy_source() {
        let body = MirFunctionBody {
            blocks: vec![
                block(
                    0,
                    vec![copy_cell_into(Local(2), Local(1))],
                    call_into_dest(Local(3), 1),
                ),
                block(1, vec![copy_into(Local(0), Local(2))], Terminator::Return),
            ],
            entry: BlockId(0),
            locals: vec![
                int_local_decl(None),
                captured_int_local_decl(Some("x")),
                int_local_decl(None),
                int_local_decl(None),
            ],
            catch_regions: vec![],
        };
        let classification =
            AnalysisResult::analyze(&body, 1, OptLevel::One).classifications[&Local(2)];
        assert_eq!(classification, LocalClassification::Real);
    }

    /// `_2 = _1.field` then a store to a field of some object, then the use.
    fn field_read_then_store(stored_field: usize) -> MirFunctionBody<'static> {
        let read_field = Statement {
            kind: StatementKind::Assign {
                destination: Place::Local(Local(2)),
                value: Rvalue::Use(Operand::Copy(Place::Field {
                    base: Box::new(Place::Local(Local(1))),
                    field: 0,
                })),
            },
            span: None,
        };
        let store = assign_int(
            Place::Field {
                base: Box::new(Place::Local(Local(1))),
                field: stored_field,
            },
            5,
        );
        MirFunctionBody {
            blocks: vec![block(
                0,
                vec![read_field, store, copy_into(Local(0), Local(2))],
                Terminator::Return,
            )],
            entry: BlockId(0),
            locals: vec![
                int_local_decl(None),
                int_local_decl(Some("obj")),
                int_local_decl(None),
            ],
            catch_regions: vec![],
        }
    }

    /// The heap model is field-sensitive: a store to the field that was read
    /// may have changed it, whatever object it went to.
    #[test]
    fn a_field_read_does_not_sink_past_a_store_to_that_field() {
        let body = field_read_then_store(0);
        assert_eq!(
            analyzed_classification(&body, Local(2)),
            LocalClassification::Real
        );
    }

    /// A store to a different field cannot change the one that was read.
    #[test]
    fn a_field_read_sinks_past_a_store_to_another_field() {
        let body = field_read_then_store(1);
        assert_eq!(
            analyzed_classification(&body, Local(2)),
            LocalClassification::Virtual
        );
    }

    /// `_1 = _2 / _3` inside a protected block, used in `use_block`; whether
    /// that block is protected by the same handler is the parameter.
    fn trapping_read_across_regions(use_protected: bool) -> MirFunctionBody<'static> {
        let divide = Statement {
            kind: StatementKind::Assign {
                destination: Place::Local(Local(1)),
                value: Rvalue::BinaryOp {
                    op: baml_compiler2_mir::BinOp::Div,
                    left: Operand::copy_local(Local(2)),
                    right: Operand::copy_local(Local(3)),
                },
            },
            span: None,
        };
        let mut body_blocks = vec![BlockId(1)];
        if use_protected {
            body_blocks.push(BlockId(2));
        }
        MirFunctionBody {
            blocks: vec![
                block(0, vec![], goto(1)),
                block(1, vec![divide], goto(2)),
                block(2, vec![copy_into(Local(0), Local(1))], Terminator::Return),
                block(3, vec![], Terminator::Return),
            ],
            entry: BlockId(0),
            locals: vec![
                int_local_decl(None),
                int_local_decl(None),
                int_local_decl(None),
                int_local_decl(None),
                int_local_decl(None),
            ],
            catch_regions: vec![CatchRegion {
                body_entry: BlockId(1),
                handler: BlockId(3),
                body_blocks,
                handler_body: vec![BlockId(3)],
                error_local: Local(4),
                stack_trace_local: None,
            }],
        }
    }

    /// A trap is delivered to the handler of the block it happens in, so an
    /// evaluation that can trap does not leave its catch region.
    #[test]
    fn a_trapping_evaluation_does_not_leave_its_catch_region() {
        let body = trapping_read_across_regions(false);
        assert_eq!(
            analyzed_classification(&body, Local(1)),
            LocalClassification::Real
        );
    }

    #[test]
    fn a_trapping_evaluation_sinks_within_its_catch_region() {
        let body = trapping_read_across_regions(true);
        assert_eq!(
            analyzed_classification(&body, Local(1)),
            LocalClassification::Virtual
        );
    }

    #[test]
    fn named_scalar_prefix_stays_in_a_slot_at_o0() {
        let mut entry = BasicBlock::new(BlockId(0));
        entry.statements.push(assign_bool(Local(1), true));
        entry.terminator = Some(Terminator::Goto { target: BlockId(1) });
        let mut result = return_local_block(BlockId(1), Local(1));
        result.statements[0].kind = StatementKind::Assign {
            destination: Place::Local(Local(0)),
            value: Rvalue::BinaryOp {
                op: baml_compiler2_mir::BinOp::Eq,
                left: Operand::copy_local(Local(1)),
                right: Operand::Constant(Constant::Bool(true)),
            },
        };
        let body = bool_body(vec![entry, result], Some("first"));
        assert_eq!(
            AnalysisResult::analyze(&body, 0, OptLevel::Zero).classifications[&Local(1)],
            LocalClassification::Real
        );
        assert_eq!(
            analyzed_classification(&body, Local(1)),
            LocalClassification::AggregateOperand
        );
    }

    #[test]
    fn nested_short_circuit_definitions_are_stack_carried() {
        let body = nested_short_circuit_body(None, false);

        assert!(is_stack_covered(&body, Local(1)));
        assert_eq!(
            analyzed_classification(&body, Local(1)),
            LocalClassification::PhiLike
        );
    }

    /// A user-named local is not special: what matters is that every edge into
    /// the use block pushes its value, and that nothing else defines it.
    #[test]
    fn named_short_circuit_local_is_stack_carried() {
        let body = nested_short_circuit_body(Some("result"), false);

        assert!(is_stack_covered(&body, Local(1)));
        assert_eq!(
            analyzed_classification(&body, Local(1)),
            LocalClassification::PhiLike
        );
    }

    #[test]
    fn reassigned_named_short_circuit_local_is_materialized() {
        let body = nested_short_circuit_body(Some("result"), true);

        assert!(!is_stack_covered(&body, Local(1)));
        assert_eq!(
            analyzed_classification(&body, Local(1)),
            LocalClassification::Real
        );
    }

    /// The stray initializer is just as unbalanced when the local is a compiler
    /// temp, so the name plays no part in rejecting it.
    #[test]
    fn reassigned_unnamed_short_circuit_local_is_materialized() {
        let body = nested_short_circuit_body(None, true);

        assert!(!is_stack_covered(&body, Local(1)));
        assert_eq!(
            analyzed_classification(&body, Local(1)),
            LocalClassification::Real
        );
    }

    /// `let x = false; if (c) { x = a && b } x` — MIR merges the short circuit's
    /// own join into the `if` join, so the `ShortCircuit`'s `join` really is the
    /// use block. The `Branch` false edge still reaches that block without
    /// pushing anything, so the local has to stay in its slot.
    fn merged_join_body(name: Option<&str>) -> MirFunctionBody<'static> {
        let destination = Local(1);

        bool_body(
            vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: vec![],
                    terminator: Some(Terminator::Branch {
                        condition: Operand::Constant(Constant::Bool(true)),
                        then_block: BlockId(1),
                        else_block: BlockId(3),
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![],
                    terminator: Some(Terminator::ShortCircuit {
                        operand: Operand::Constant(Constant::Bool(true)),
                        kind: baml_compiler2_mir::ShortCircuitKind::And,
                        destination: Place::Local(destination),
                        eval_rhs: BlockId(2),
                        join: BlockId(3),
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(2),
                    statements: vec![assign_bool(destination, true)],
                    terminator: Some(Terminator::Goto { target: BlockId(3) }),
                    span: None,
                    terminator_span: None,
                },
                return_local_block(BlockId(3), destination),
            ],
            name,
        )
    }

    #[test]
    fn short_circuit_join_shared_with_a_branch_edge_is_materialized() {
        assert!(!is_stack_covered(&merged_join_body(Some("x")), Local(1)));
        assert!(!is_stack_covered(&merged_join_body(None), Local(1)));
    }

    /// One predecessor short-circuits into the join, the other assigns and
    /// falls through. Neither of the two predicates this replaced accepted the
    /// mix on its own.
    #[test]
    fn mixed_short_circuit_and_assignment_predecessors_are_stack_carried() {
        let destination = Local(1);
        let body = bool_body(
            vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: vec![],
                    terminator: Some(Terminator::Branch {
                        condition: Operand::Constant(Constant::Bool(true)),
                        then_block: BlockId(1),
                        else_block: BlockId(3),
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![],
                    terminator: Some(Terminator::ShortCircuit {
                        operand: Operand::Constant(Constant::Bool(true)),
                        kind: baml_compiler2_mir::ShortCircuitKind::And,
                        destination: Place::Local(destination),
                        eval_rhs: BlockId(2),
                        join: BlockId(4),
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(2),
                    statements: vec![assign_bool(destination, true)],
                    terminator: Some(Terminator::Goto { target: BlockId(4) }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(3),
                    statements: vec![assign_bool(destination, false)],
                    terminator: Some(Terminator::Goto { target: BlockId(4) }),
                    span: None,
                    terminator_span: None,
                },
                return_local_block(BlockId(4), destination),
            ],
            Some("x"),
        );

        assert!(is_stack_covered(&body, Local(1)));
    }

    /// `a && f(b)`: the short circuit joins at bb2, and the rhs block reaches
    /// that same join through `rhs`, a terminator that defines the destination.
    fn short_circuit_over_call_body(
        name: Option<&str>,
        rhs: Terminator<'static>,
    ) -> MirFunctionBody<'static> {
        let destination = Local(1);

        bool_body(
            vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: vec![],
                    terminator: Some(Terminator::ShortCircuit {
                        operand: Operand::Constant(Constant::Bool(true)),
                        kind: baml_compiler2_mir::ShortCircuitKind::And,
                        destination: Place::Local(destination),
                        eval_rhs: BlockId(1),
                        join: BlockId(2),
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![],
                    terminator: Some(rhs),
                    span: None,
                    terminator_span: None,
                },
                return_local_block(BlockId(2), destination),
            ],
            name,
        )
    }

    fn call_into(target: BlockId, unwind: Option<BlockId>) -> Terminator<'static> {
        Terminator::Call {
            argument_layout: None,
            callee: Operand::Constant(Constant::Null),
            args: vec![],
            ntypeargs: 0,
            runtime_id: None,
            destination: Place::Local(Local(1)),
            target,
            unwind,
        }
    }

    fn virtual_call_into(target: BlockId) -> Terminator<'static> {
        Terminator::VirtualCall {
            argument_layout: None,
            iface: baml_type::TyTemplateInterface::new(
                baml_type::TypeName::from_dotted_path("baml.ops.Equals"),
                Box::new([]),
                Box::new([]),
            ),
            method: "eq".to_string(),
            args: vec![],
            ntypeargs: 0,
            runtime_id: None,
            destination: Place::Local(Local(1)),
            target,
            unwind: None,
        }
    }

    #[test]
    fn virtual_call_result_carries_only_on_single_entry_continuations() {
        let mut entry = BasicBlock::new(BlockId(0));
        entry.terminator = Some(virtual_call_into(BlockId(1)));
        let body = bool_body(
            vec![entry.clone(), return_local_block(BlockId(1), Local(1))],
            None,
        );
        assert_eq!(
            analyzed_classification(&body, Local(1)),
            LocalClassification::CallResultImmediate
        );

        let mut header = BasicBlock::new(BlockId(1));
        header.terminator = Some(Terminator::Branch {
            condition: Operand::copy_local(Local(1)),
            then_block: BlockId(2),
            else_block: BlockId(3),
        });
        let mut backedge = BasicBlock::new(BlockId(2));
        backedge.terminator = Some(Terminator::Goto { target: BlockId(1) });
        let mut exit = BasicBlock::new(BlockId(3));
        exit.statements.push(assign_bool(Local(0), false));
        exit.terminator = Some(Terminator::Return);
        let loop_body = bool_body(vec![entry, header, backedge, exit], None);
        assert_eq!(
            analyzed_classification(&loop_body, Local(1)),
            LocalClassification::Real
        );
    }

    #[test]
    fn call_result_with_a_prior_definition_is_not_carried() {
        for call in [call_into(BlockId(1), None), virtual_call_into(BlockId(1))] {
            let mut entry = BasicBlock::new(BlockId(0));
            entry.statements.push(assign_bool(Local(1), false));
            entry.terminator = Some(call);
            let body = bool_body(vec![entry, return_local_block(BlockId(1), Local(1))], None);
            assert_eq!(
                analyzed_classification(&body, Local(1)),
                LocalClassification::Real
            );
        }
    }

    /// `a && f(b)` and `a && (b == c)`: the call opcode leaves its result on the
    /// stack and jumps to the join, exactly like an assignment that falls
    /// through. Both are common enough that materializing them would cost the
    /// stdlib's comparison operators a store/load pair.
    #[test]
    fn short_circuit_joining_a_call_result_is_stack_carried() {
        for name in [None, Some("ok")] {
            let call = short_circuit_over_call_body(name, call_into(BlockId(2), None));
            assert!(is_stack_covered(&call, Local(1)));

            let virtual_call = short_circuit_over_call_body(name, virtual_call_into(BlockId(2)));
            assert!(is_stack_covered(&virtual_call, Local(1)));
        }
    }

    /// The throwing edge reaches the join without the call ever pushing a
    /// result, so the join cannot assume anything is on the stack.
    #[test]
    fn call_that_unwinds_into_the_join_is_materialized() {
        let body = short_circuit_over_call_body(None, call_into(BlockId(2), Some(BlockId(2))));

        assert!(!is_stack_covered(&body, Local(1)));
    }

    /// `Await` suspends the engine and its opcode re-executes across the
    /// suspend, so its result is not carried even though it lands on the stack
    /// the same way a call's does.
    #[test]
    fn awaited_result_in_the_join_is_materialized() {
        let body = short_circuit_over_call_body(
            None,
            Terminator::Await {
                future: Place::Local(Local(0)),
                destination: Place::Local(Local(1)),
                target: BlockId(2),
                unwind: None,
            },
        );

        assert!(!is_stack_covered(&body, Local(1)));
    }

    /// The entry block is entered on every call without traversing an edge, so
    /// a predecessor set that looks complete is not. Contrived — a real MIR
    /// entry block is never a loop header — but it is the shape the guard
    /// exists for, and nothing else in the predicate would reject it.
    #[test]
    fn entry_block_as_the_join_is_materialized() {
        let destination = Local(1);
        let body = bool_body(
            vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: vec![Statement {
                        kind: StatementKind::Assign {
                            destination: Place::Local(Local(0)),
                            value: Rvalue::Use(Operand::copy_local(destination)),
                        },
                        span: None,
                    }],
                    terminator: Some(Terminator::Branch {
                        condition: Operand::Constant(Constant::Bool(true)),
                        then_block: BlockId(1),
                        else_block: BlockId(2),
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![assign_bool(destination, true)],
                    terminator: Some(Terminator::Goto { target: BlockId(0) }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(2),
                    statements: vec![assign_bool(destination, false)],
                    terminator: Some(Terminator::Goto { target: BlockId(0) }),
                    span: None,
                    terminator_span: None,
                },
            ],
            Some("x"),
        );

        assert!(!is_stack_covered(&body, Local(1)));
    }

    /// The VM enters a handler on unwind rather than over an edge, so a block
    /// the walk would otherwise pass through as an empty passthrough cannot be
    /// proven covered once it is one.
    #[test]
    fn catch_handler_inside_the_coverage_walk_is_materialized() {
        let mut body = nested_short_circuit_body(None, false);

        // bb3 is the chain's inner join, the empty passthrough arm (c) recurses
        // through. Everything below is unchanged except that it is now a
        // handler, so the region is the only reason the answer flips.
        assert!(is_stack_covered(&body, Local(1)));

        body.catch_regions.push(CatchRegion {
            body_entry: BlockId(0),
            handler: BlockId(3),
            body_blocks: vec![BlockId(0)],
            handler_body: vec![BlockId(3)],
            error_local: Local(0),
            stack_trace_local: None,
        });

        assert!(!is_stack_covered(&body, Local(1)));
    }

    /// The classic phi-like diamond: both arms assign and fall through.
    fn diamond_body(with_prior_definition: bool) -> MirFunctionBody<'static> {
        let destination = Local(1);
        let prior_definition = with_prior_definition.then(|| assign_bool(destination, false));

        bool_body(
            vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: prior_definition.into_iter().collect(),
                    terminator: Some(Terminator::Branch {
                        condition: Operand::Constant(Constant::Bool(true)),
                        then_block: BlockId(1),
                        else_block: BlockId(2),
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![assign_bool(destination, true)],
                    terminator: Some(Terminator::Goto { target: BlockId(3) }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(2),
                    statements: vec![assign_bool(destination, false)],
                    terminator: Some(Terminator::Goto { target: BlockId(3) }),
                    span: None,
                    terminator_span: None,
                },
                return_local_block(BlockId(3), destination),
            ],
            Some("x"),
        )
    }

    #[test]
    fn assignment_in_every_predecessor_is_stack_carried() {
        assert!(is_stack_covered(&diamond_body(false), Local(1)));
    }

    /// `let x = 0; if (c) { x = 1 } else { x = 2 }; use(x)` — the initializer is
    /// emitted as a push that nothing pops, so inside a loop it grew the operand
    /// stack once per iteration.
    #[test]
    fn diamond_with_a_definition_outside_the_predecessors_is_materialized() {
        assert!(!is_stack_covered(&diamond_body(true), Local(1)));
    }

    /// A single static use in a CFG cycle represents repeated dynamic uses.
    /// Sinking the array allocation from block 0 into the call in block 1 would
    /// allocate a fresh array on every trip around the cycle.
    #[test]
    fn repeated_identity_allocation_is_not_virtualized() {
        let array = Local(1);
        let call_result = Local(2);
        let body = MirFunctionBody {
            blocks: vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: vec![Statement {
                        kind: StatementKind::Assign {
                            destination: Place::Local(array),
                            value: Rvalue::Array(
                                baml_type::TyTemplate::from(baml_type::RealizedTy::int()),
                                vec![],
                            ),
                        },
                        span: None,
                    }],
                    terminator: Some(Terminator::Goto { target: BlockId(1) }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![],
                    terminator: Some(Terminator::Call {
                        argument_layout: None,
                        callee: Operand::Constant(Constant::Null),
                        args: vec![Operand::copy_local(array)],
                        ntypeargs: 0,
                        runtime_id: None,
                        destination: Place::Local(call_result),
                        target: BlockId(2),
                        unwind: None,
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(2),
                    statements: vec![],
                    terminator: Some(Terminator::Goto { target: BlockId(1) }),
                    span: None,
                    terminator_span: None,
                },
            ],
            entry: BlockId(0),
            locals: vec![
                int_local_decl(None),
                int_list_local_decl(Some("items")),
                int_local_decl(Some("result")),
            ],
            catch_regions: vec![],
        };

        let analysis = AnalysisResult::analyze(&body, 0, OptLevel::One);
        assert_eq!(
            analysis.classifications.get(&array),
            Some(&LocalClassification::Real)
        );
    }

    /// The identity guard is cycle-specific: a single cross-block use that
    /// cannot repeat without re-running the definition remains virtualizable.
    #[test]
    fn non_repeating_identity_allocation_stays_virtualized() {
        let array = Local(1);
        let body = MirFunctionBody {
            blocks: vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: vec![Statement {
                        kind: StatementKind::Assign {
                            destination: Place::Local(array),
                            value: Rvalue::Array(
                                baml_type::TyTemplate::from(baml_type::RealizedTy::int()),
                                vec![],
                            ),
                        },
                        span: None,
                    }],
                    terminator: Some(Terminator::Goto { target: BlockId(1) }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![Statement {
                        kind: StatementKind::Assign {
                            destination: Place::Local(Local(0)),
                            value: Rvalue::Use(Operand::copy_local(array)),
                        },
                        span: None,
                    }],
                    terminator: Some(Terminator::Return),
                    span: None,
                    terminator_span: None,
                },
            ],
            entry: BlockId(0),
            locals: vec![
                int_list_local_decl(None),
                int_list_local_decl(Some("items")),
            ],
            catch_regions: vec![],
        };

        let analysis = AnalysisResult::analyze(&body, 0, OptLevel::One);
        assert_eq!(
            analysis.classifications.get(&array),
            Some(&LocalClassification::Virtual)
        );
    }

    /// `let n = xs.length()` followed by `n`: a length lives with the elements,
    /// and nothing between the two statements touches them.
    fn length_read(between: Terminator<'static>) -> MirFunctionBody<'static> {
        let arr = Local(1);
        let len = Local(2);
        MirFunctionBody {
            blocks: vec![
                block(
                    0,
                    vec![
                        Statement {
                            kind: StatementKind::Assign {
                                destination: Place::Local(arr),
                                value: Rvalue::Use(Operand::Constant(Constant::Null)),
                            },
                            span: None,
                        },
                        Statement {
                            kind: StatementKind::Assign {
                                destination: Place::Local(len),
                                value: Rvalue::Len(Place::Local(arr)),
                            },
                            span: None,
                        },
                    ],
                    between,
                ),
                block(1, vec![copy_into(Local(0), len)], Terminator::Return),
            ],
            entry: BlockId(0),
            locals: vec![
                int_local_decl(None),
                int_list_local_decl(Some("arr")),
                int_local_decl(Some("n")),
                int_local_decl(None),
            ],
            catch_regions: vec![],
        }
    }

    #[test]
    fn an_unmutated_length_read_sinks() {
        let body = length_read(goto(1));
        assert_eq!(
            analyzed_classification(&body, Local(2)),
            LocalClassification::Virtual
        );
    }

    /// A call may push: the length must be taken before it.
    #[test]
    fn a_length_read_does_not_sink_across_a_call() {
        let body = length_read(call_into_dest(Local(3), 1));
        assert_eq!(
            analyzed_classification(&body, Local(2)),
            LocalClassification::Real
        );
    }
}
