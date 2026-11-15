//! Redundancy elimination: evaluations and jumps whose outcome the body
//! already has.
//!
//! - [`reuse_available_values`] replaces a computation with a copy of the
//!   local that already holds its value (common-subexpression elimination).
//! - [`fold_negated_branches`] branches on a condition instead of on its
//!   negation.
//! - [`thread_returns`] returns where a block would jump to a bare `return`.
//!
//! None of them moves an evaluation. Each removes one whose result is known
//! at that point, so the order of the traps and effects that remain is the
//! order the program had.

use std::collections::HashMap;

use crate::{
    BlockId, Constant, Local, MirFunctionBody, Operand, Place, RuntimeTy, Rvalue, StatementKind,
    Terminator, UnaryOp,
    memory::{self, Resources},
};

/// The redundancy passes, then loop rotation over what they left.
///
/// Runs once, on a body the cleanup passes have finished with. Rotation has
/// to come last: the trap proofs dead-code elimination relies on bound a
/// loop's counter from the test at the top of the loop, and are weaker once
/// the test sits at the bottom.
pub(super) fn optimize(body: &mut MirFunctionBody<'_>, arity: usize) {
    reuse_available_values(body);
    fold_negated_branches(body);
    thread_returns(body);
    // Sweep the temporaries reuse orphaned and the return blocks nothing
    // jumps to any more, so a loop test is copied clean.
    super::eliminate_dead_blocks(body);
    super::eliminate_dead_locals(body, arity);
    super::loops::rotate_loops(body, arity);
}

// ============================================================================
// Available values
// ============================================================================

/// An operand of a reusable computation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Value {
    Place(Place),
    Int(i64),
    /// By bit pattern: `0.0` and `-0.0` are different operands.
    Float(u64),
    Bool(bool),
    Null,
    String(String),
}

/// A computation whose result depends only on what it reads: evaluated twice
/// with nothing it reads written in between, it yields the same value and, if
/// the first evaluation did not trap, neither does the second.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Expression {
    /// The operator by discriminant: operand types decide which operation it
    /// is, and the same operands have the same types.
    Binary(u8, Value, Value),
    Unary(u8, Value),
    /// A read through a field or index projection.
    Read(Place),
    Len(Place),
}

/// Instructions the VM runs to evaluate a place: one load per local, one more
/// per projection.
fn place_cost(place: &Place) -> usize {
    match place {
        Place::Local(_) | Place::Capture(_) | Place::Deref(_) => 1,
        Place::Field { base, .. } => place_cost(base) + 1,
        Place::Index { base, .. } => place_cost(base) + 2,
    }
}

impl Value {
    fn cost(&self) -> usize {
        match self {
            Self::Place(place) => place_cost(place),
            _ => 1,
        }
    }

    fn reads(&self, out: &mut Resources) {
        if let Self::Place(place) = self {
            memory::place_reads(place, out);
        }
    }
}

impl Expression {
    /// Instructions the VM runs to evaluate this where it is used.
    fn cost(&self) -> usize {
        match self {
            Self::Binary(_, left, right) => left.cost() + right.cost() + 1,
            Self::Unary(_, operand) => operand.cost() + 1,
            Self::Read(place) => place_cost(place),
            Self::Len(place) => place_cost(place) + 1,
        }
    }

    /// The storage the value depends on. Whether the evaluation can trap is
    /// left out: that does not decide whether its value is still current.
    fn reads(&self) -> Resources {
        let mut out = Resources::default();
        match self {
            Self::Binary(_, left, right) => {
                left.reads(&mut out);
                right.reads(&mut out);
            }
            Self::Unary(_, operand) => operand.reads(&mut out),
            Self::Read(place) => memory::place_reads(place, &mut out),
            Self::Len(place) => {
                memory::place_reads(place, &mut out);
                // A length lives with the elements: a push changes it.
                out.elements = true;
            }
        }
        out
    }
}

/// The expressions of a body, numbered, with what each reads.
#[derive(Default)]
struct Expressions {
    ids: HashMap<Expression, usize>,
    reads: Vec<Resources>,
    costs: Vec<usize>,
}

impl Expressions {
    fn intern(&mut self, expression: Expression) -> usize {
        if let Some(&id) = self.ids.get(&expression) {
            return id;
        }
        let id = self.reads.len();
        self.reads.push(expression.reads());
        self.costs.push(expression.cost());
        self.ids.insert(expression, id);
        id
    }
}

/// What a program point knows about the values in locals.
#[derive(Clone, Default, PartialEq)]
struct Available {
    /// Expressions whose value a local holds: the local was assigned the
    /// expression on every path here, and nothing the expression reads, nor
    /// the local, has been written since.
    values: HashMap<usize, Local>,
    /// Locals holding the same value as an earlier one: a copy of it, or a
    /// recomputation of what it holds. Lowering reads a named local through a
    /// fresh temporary at every use, so two evaluations of one source
    /// expression only look alike once those are seen through.
    same_as: HashMap<Local, Local>,
}

impl Available {
    fn root(&self, local: Local) -> Local {
        self.same_as.get(&local).copied().unwrap_or(local)
    }

    /// `place` with every local replaced by the earliest one holding the same
    /// value. `None` for a place through a cell: a closure sharing the cell
    /// is not visible to this body.
    fn place(&self, place: &Place) -> Option<Place> {
        Some(match place {
            Place::Local(local) => Place::Local(self.root(*local)),
            Place::Field { base, field } => Place::Field {
                base: Box::new(self.place(base)?),
                field: *field,
            },
            Place::Index { base, index, kind } => Place::Index {
                base: Box::new(self.place(base)?),
                index: self.root(*index),
                kind: *kind,
            },
            Place::Capture(_) | Place::Deref(_) => return None,
        })
    }

    fn value(&self, operand: &Operand<'_>) -> Option<Value> {
        Some(match operand {
            Operand::Copy(place) => Value::Place(self.place(place)?),
            Operand::Constant(Constant::Int(n)) => Value::Int(*n),
            Operand::Constant(Constant::Float(n)) => Value::Float(n.to_bits()),
            Operand::Constant(Constant::Bool(b)) => Value::Bool(*b),
            Operand::Constant(Constant::Null) => Value::Null,
            Operand::Constant(Constant::String(s)) => Value::String(s.clone()),
            // A move consumes its source, and the other constants are
            // references this pass has no reason to compare.
            Operand::Move(_) | Operand::Constant(_) => return None,
        })
    }

    fn expression(&self, rvalue: &Rvalue<'_>) -> Option<Expression> {
        match rvalue {
            Rvalue::BinaryOp { op, left, right } => Some(Expression::Binary(
                *op as u8,
                self.value(left)?,
                self.value(right)?,
            )),
            Rvalue::UnaryOp { op, operand } => {
                Some(Expression::Unary(*op as u8, self.value(operand)?))
            }
            Rvalue::Use(Operand::Copy(place @ (Place::Field { .. } | Place::Index { .. }))) => {
                Some(Expression::Read(self.place(place)?))
            }
            Rvalue::Len(place) => Some(Expression::Len(self.place(place)?)),
            _ => None,
        }
    }

    /// Forget everything a write invalidates.
    fn kill(&mut self, expressions: &Expressions, mut written: Resources) {
        written.order = false;
        if written.is_empty() {
            return;
        }
        self.values.retain(|&id, holder| {
            !written.locals.contains(holder) && !expressions.reads[id].intersects(&written)
        });
        self.same_as.retain(|local, source| {
            !written.locals.contains(local) && !written.locals.contains(source)
        });
    }

    /// Keep only what `other` agrees on; whether anything was dropped.
    fn meet(&mut self, other: &Self) -> bool {
        let before = (self.values.len(), self.same_as.len());
        self.values
            .retain(|id, holder| other.values.get(id) == Some(holder));
        self.same_as
            .retain(|local, source| other.same_as.get(local) == Some(source));
        before != (self.values.len(), self.same_as.len())
    }
}

/// What [`walk_block`] reports about an assignment.
enum Event {
    /// The value is one `holder` already has.
    Reuse {
        statement: usize,
        expression: usize,
        holder: Local,
        /// Whether `holder` took the value earlier in this same block.
        same_block: bool,
    },
    /// The destination becomes the holder of the value.
    Hold {
        statement: usize,
        expression: usize,
        holder: Local,
    },
}

/// Walk one block from its entry state to its exit state.
fn walk_block(
    body: &MirFunctionBody<'_>,
    model: memory::ClobberModel,
    expressions: &mut Expressions,
    block: BlockId,
    mut available: Available,
    mut report: impl FnMut(Event),
) -> Available {
    let block = &body.blocks[block.0];
    let mut held_here: Vec<usize> = Vec::new();
    for (statement, assignment) in block.statements.iter().enumerate() {
        // What the assignment computes, judged before its own write lands.
        let (expression, copied) = match &assignment.kind {
            StatementKind::Assign { value, .. } => (
                available
                    .expression(value)
                    .map(|expression| expressions.intern(expression)),
                match value {
                    Rvalue::Use(Operand::Copy(Place::Local(source))) => {
                        Some(available.root(*source))
                    }
                    _ => None,
                },
            ),
            _ => (None, None),
        };
        let held = expression.and_then(|id| Some((id, *available.values.get(&id)?)));
        available.kill(
            expressions,
            memory::statement_clobbers(body, model, &assignment.kind),
        );
        let StatementKind::Assign { destination, .. } = &assignment.kind else {
            continue;
        };
        if let Some((expression, holder)) = held
            // An assignment back into the holder has nothing to copy from
            // but itself.
            && *destination != Place::Local(holder)
        {
            report(Event::Reuse {
                statement,
                expression,
                holder,
                same_block: held_here.contains(&expression),
            });
        }
        let Place::Local(local) = destination else {
            continue;
        };
        debug_assert!(
            !body.locals[local.0].is_captured,
            "bare store to captured {local}"
        );
        // The destination now holds what the holder or the copied local
        // holds, unless this assignment is the one that overwrote it.
        let same = held.map(|(_, holder)| holder).or(copied);
        if let Some(source) = same
            && source != *local
        {
            available.same_as.insert(*local, source);
        } else if let Some(expression) = expression
            && !expressions.reads[expression].locals.contains(local)
        {
            available.values.insert(expression, *local);
            held_here.push(expression);
            report(Event::Hold {
                statement,
                expression,
                holder: *local,
            });
        }
    }
    if let Some(terminator) = &block.terminator {
        available.kill(expressions, memory::terminator_clobbers(model, terminator));
    }
    available
}

/// Passes over the body before the analysis gives up. Each pass carries
/// facts one loop nesting level further, so real bodies settle in a few.
const MAX_PASSES: usize = 16;

/// What is available on entry to each block, or `None` for a block nothing
/// reaches. `None` overall when the analysis did not settle.
///
/// A block's entry is the meet of its predecessors' exits, recomputed from
/// scratch whenever one of them changes rather than narrowed in place: which
/// expression an assignment computes depends on the copies known at that
/// point, so a fact can reappear under another name as the copies settle.
/// Any state this stops at is a fixed point of the equations, and a fixed
/// point is sound by induction along each execution: the entry starts with
/// nothing, and every step preserves what its block's entry state claims.
fn available_on_entry(
    body: &MirFunctionBody<'_>,
    model: memory::ClobberModel,
    expressions: &mut Expressions,
) -> Option<Vec<Option<Available>>> {
    let blocks = body.blocks.len();
    // A handler is entered from anywhere in the blocks it protects, and the
    // entry block from outside the body: both start with nothing.
    let is_root = |block: usize| block == body.entry.0 || body.blocks[block].landing.is_some();
    let mut predecessors = vec![Vec::new(); blocks];
    for block in &body.blocks {
        for successor in block.terminator.iter().flat_map(Terminator::successors) {
            predecessors[successor.0].push(block.id.0);
        }
    }

    let loop_headers = loop_headers(body);

    let mut entries: Vec<Option<Available>> = vec![None; blocks];
    let mut exits: Vec<Option<Available>> = vec![None; blocks];
    for _ in 0..MAX_PASSES {
        let mut changed = false;
        for block in 0..blocks {
            let entry = if is_root(block) {
                Some(Available::default())
            } else {
                // A predecessor not reached yet constrains nothing so far.
                let mut reached = predecessors[block]
                    .iter()
                    .filter_map(|&pred| exits[pred].as_ref());
                reached.next().cloned().map(|mut entry| {
                    for exit in reached {
                        entry.meet(exit);
                    }
                    // A heap read is reused only within one trip. Carried
                    // around the back edge it would stand for the reads of
                    // every later trip, and a loop waiting on another task's
                    // write would never see it land.
                    if loop_headers[block] {
                        entry.values.retain(|&id, _| {
                            let reads = &expressions.reads[id];
                            !reads.heap
                                && !reads.elements
                                && reads.fields.is_empty()
                                && reads.cells.is_empty()
                        });
                    }
                    entry
                })
            };
            if entry.is_some() && entry != entries[block] {
                let exit = walk_block(
                    body,
                    model,
                    expressions,
                    BlockId(block),
                    entry.clone().expect("checked above"),
                    |_| {},
                );
                entries[block] = entry;
                exits[block] = Some(exit);
                changed = true;
            }
        }
        if !changed {
            return Some(entries);
        }
    }
    None
}

/// Which blocks head a loop: some predecessor is a block they dominate.
fn loop_headers(body: &MirFunctionBody<'_>) -> Vec<bool> {
    let entry = body.entry.0;
    let succs: Vec<Vec<usize>> = body
        .blocks
        .iter()
        .map(|block| {
            block
                .terminator
                .iter()
                .flat_map(Terminator::successors)
                .chain(block.unwind)
                .map(|b| b.0)
                .collect()
        })
        .collect();
    let rpo = crate::reducible::reverse_postorder(entry, &succs);
    let idom = crate::reducible::immediate_dominators(entry, &succs, &rpo);
    let mut headers = vec![false; body.blocks.len()];
    for &block in &rpo {
        for &successor in &succs[block] {
            // Walk up from `block`: a back edge ends at one of its dominators.
            let mut dominator = block;
            loop {
                if dominator == successor {
                    headers[successor] = true;
                    break;
                }
                if dominator == entry {
                    break;
                }
                dominator = idom[dominator];
            }
        }
    }
    headers
}

/// Whether the value `holder` takes at `statement` is consumed straight off
/// the stack: it is the first operand of the next evaluation the block emits.
/// The emitter then keeps it in a slot with one store-and-keep instruction.
fn feeds_next_evaluation(block: &crate::BasicBlock<'_>, statement: usize, holder: Local) -> bool {
    let reads_holder = |operand: &Operand<'_>| {
        matches!(
            operand,
            Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local))
                if *local == holder
        )
    };
    for next in &block.statements[statement + 1..] {
        return match &next.kind {
            // A copy of another local emits nothing where it stands: its one
            // reader loads the source.
            StatementKind::Assign {
                destination: Place::Local(_),
                value: Rvalue::Use(Operand::Copy(Place::Local(source))),
            } if *source != holder => continue,
            StatementKind::Assign {
                value:
                    Rvalue::Use(operand)
                    | Rvalue::UnaryOp { operand, .. }
                    | Rvalue::BinaryOp { left: operand, .. },
                ..
            } => reads_holder(operand),
            _ => false,
        };
    }
    match &block.terminator {
        Some(
            Terminator::Branch {
                condition: operand, ..
            }
            | Terminator::Switch {
                discriminant: operand,
                ..
            },
        ) => reads_holder(operand),
        _ => false,
    }
}

/// Weights are in half instructions, so a reuse the holder's block does not
/// guarantee can count for half of one that it does.
const SAME_BLOCK_WEIGHT: usize = 2;
const OTHER_BLOCK_WEIGHT: usize = 1;
/// What keeping the holder in a slot costs, in the same unit, when its value
/// is consumed straight off the stack: one store-and-keep.
const HOLDER_ON_STACK_COST: usize = 2;
/// And otherwise: a store, a load at its own use, and the load pairing that
/// load breaks up.
const HOLDER_IN_SLOT_COST: usize = 6;

/// Replace a computation with a copy of the local that already holds its
/// value, where that saves instructions.
///
/// A forward available-expressions analysis, met by intersection: an
/// expression survives a join only when every predecessor has it in the same
/// local. Writes are judged by [`memory`], so a store to a field forgets every
/// read of that field on any object, and a call, await, or spawn forgets every
/// heap read: another task may have run. Between two reads with nothing of
/// that kind in between, no step of this task lets another task's write
/// become visible, so seeing the first value again is an outcome the program
/// already allowed.
fn reuse_available_values(body: &mut MirFunctionBody<'_>) {
    /// The reuse sites of one (expression, holder) pair.
    struct Group {
        /// `(block, statement, weight)` of each reuse.
        sites: Vec<(usize, usize, usize)>,
        /// Whether every assignment that makes the local the holder is
        /// consumed straight off the stack.
        holder_on_stack: bool,
    }

    let model = memory::ClobberModel::for_body(body);
    let mut expressions = Expressions::default();
    let Some(entries) = available_on_entry(body, model, &mut expressions) else {
        return;
    };

    let mut groups: HashMap<(usize, Local), Group> = HashMap::new();
    let mut holds: Vec<(usize, usize, usize, Local)> = Vec::new();
    for (block, entry) in entries.into_iter().enumerate() {
        let Some(entry) = entry else {
            continue;
        };
        walk_block(
            body,
            model,
            &mut expressions,
            BlockId(block),
            entry,
            |event| match event {
                Event::Reuse {
                    statement,
                    expression,
                    holder,
                    same_block,
                } => {
                    let weight = if same_block {
                        SAME_BLOCK_WEIGHT
                    } else {
                        OTHER_BLOCK_WEIGHT
                    };
                    groups
                        .entry((expression, holder))
                        .or_insert_with(|| Group {
                            sites: Vec::new(),
                            holder_on_stack: true,
                        })
                        .sites
                        .push((block, statement, weight));
                }
                Event::Hold {
                    statement,
                    expression,
                    holder,
                } => holds.push((block, statement, expression, holder)),
            },
        );
    }
    for (block, statement, expression, holder) in holds {
        if let Some(group) = groups.get_mut(&(expression, holder)) {
            group.holder_on_stack &= feeds_next_evaluation(&body.blocks[block], statement, holder);
        }
    }

    let reuse = |body: &mut MirFunctionBody<'_>, block: usize, statement: usize, holder| {
        if let StatementKind::Assign { value, .. } =
            &mut body.blocks[block].statements[statement].kind
        {
            *value = Rvalue::Use(Operand::Copy(Place::Local(holder)));
        }
    };
    let mut declined = Vec::new();
    for ((expression, holder), group) in groups {
        // Each reuse turns an evaluation into one load. Against that, the
        // holder now has to live in a slot. A reuse in a later block may sit
        // on a path the program rarely takes, so it counts for half.
        let saved: usize = group
            .sites
            .iter()
            .map(|(_, _, weight)| weight * (expressions.costs[expression] - 1))
            .sum();
        let cost = if group.holder_on_stack {
            HOLDER_ON_STACK_COST
        } else {
            HOLDER_IN_SLOT_COST
        };
        for (block, statement, _) in group.sites {
            if saved > cost {
                reuse(body, block, statement, holder);
            } else {
                declined.push((block, statement, holder));
            }
        }
    }

    // A declined reuse whose own result fed only reuses that were taken now
    // computes a value nothing reads. Left as it is, an evaluation that can
    // trap would have to be kept and popped; as a copy it is simply dead.
    loop {
        let uses = super::count_local_uses(body);
        let before = declined.len();
        declined.retain(|&(block, statement, holder)| {
            let orphaned = matches!(
                &body.blocks[block].statements[statement].kind,
                StatementKind::Assign {
                    destination: Place::Local(local),
                    ..
                } if uses[local.0] == 0
            );
            if orphaned {
                reuse(body, block, statement, holder);
            }
            !orphaned
        });
        if declined.len() == before {
            break;
        }
    }
}

// ============================================================================
// Branches
// ============================================================================

/// `_t = !c; branch _t -> [a, b]` becomes `branch c -> [b, a]` when the
/// negation is the block's last statement and feeds nothing else.
fn fold_negated_branches(body: &mut MirFunctionBody<'_>) {
    let defs = super::count_local_defs(body);
    let uses = super::count_local_uses(body);
    for block in &mut body.blocks {
        let Some(Terminator::Branch {
            condition: Operand::Copy(Place::Local(negated)) | Operand::Move(Place::Local(negated)),
            then_block,
            else_block,
        }) = &block.terminator
        else {
            continue;
        };
        let Some(StatementKind::Assign {
            destination: Place::Local(destination),
            value:
                Rvalue::UnaryOp {
                    op: UnaryOp::Not,
                    operand: operand @ Operand::Copy(Place::Local(source)),
                },
        }) = block.statements.last().map(|statement| &statement.kind)
        else {
            continue;
        };
        // `!` of a bool is the only negation a branch on the operand itself
        // decides the same way.
        if destination != negated
            || defs[negated.0] != 1
            || uses[negated.0] != 1
            || body.locals[negated.0].name.is_some()
            || !matches!(body.locals[source.0].ty, RuntimeTy::Bool)
        {
            continue;
        }
        let folded = Terminator::Branch {
            condition: operand.clone(),
            then_block: *else_block,
            else_block: *then_block,
        };
        block.terminator = Some(folded);
        block.statements.pop();
    }
}

// ============================================================================
// Returns
// ============================================================================

/// A `goto` to a block that only returns becomes the `return` itself, saving
/// the jump. The return block stays for the predecessors that fall into it.
fn thread_returns(body: &mut MirFunctionBody<'_>) {
    for index in 0..body.blocks.len() {
        let Some(Terminator::Goto { target }) = body.blocks[index].terminator else {
            continue;
        };
        let block = &body.blocks[index];
        let returning = &body.blocks[target.0];
        // A handler is entered by the VM, never by falling out of a `goto`;
        // the handler and shield tables are per block, so both blocks must
        // already agree on them.
        if !returning.statements.is_empty()
            || !matches!(returning.terminator, Some(Terminator::Return))
            || returning.landing.is_some()
            || returning.unwind != block.unwind
            || returning.handling != block.handling
            || returning.shielded != block.shielded
        {
            continue;
        }
        let span = returning.terminator_span;
        let block = &mut body.blocks[index];
        block.terminator = Some(Terminator::Return);
        block.terminator_span = span.or(block.terminator_span);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BasicBlock, BinOp, Landing, LocalDecl, Statement};

    fn local(ty: RuntimeTy) -> LocalDecl {
        LocalDecl {
            name: None,
            ty,
            span: None,
            scope_span: None,
            is_captured: false,
        }
    }

    fn body(blocks: Vec<BasicBlock<'static>>, locals: usize) -> MirFunctionBody<'static> {
        MirFunctionBody {
            blocks,
            entry: BlockId(0),
            locals: (0..locals).map(|_| local(RuntimeTy::int())).collect(),
        }
    }

    fn block(
        id: usize,
        statements: Vec<Statement<'static>>,
        terminator: Terminator<'static>,
    ) -> BasicBlock<'static> {
        let mut block = BasicBlock::new(BlockId(id));
        block.statements = statements;
        block.terminator = Some(terminator);
        block
    }

    fn assign(destination: Place, value: Rvalue<'static>) -> Statement<'static> {
        Statement {
            kind: StatementKind::Assign { destination, value },
            span: None,
        }
    }

    fn set(destination: usize, value: Rvalue<'static>) -> Statement<'static> {
        assign(Place::Local(Local(destination)), value)
    }

    fn copy(source: usize) -> Rvalue<'static> {
        Rvalue::Use(Operand::copy_local(Local(source)))
    }

    fn int(value: i64) -> Rvalue<'static> {
        Rvalue::Use(Operand::Constant(Constant::Int(value)))
    }

    /// `_left / _right`: a computation that can trap.
    fn divide(left: usize, right: usize) -> Rvalue<'static> {
        Rvalue::BinaryOp {
            op: BinOp::Div,
            left: Operand::copy_local(Local(left)),
            right: Operand::copy_local(Local(right)),
        }
    }

    /// `_base.0.1`: a heap read two projections deep.
    fn nested_field(base: usize) -> Place {
        Place::Field {
            base: Box::new(Place::Field {
                base: Box::new(Place::Local(Local(base))),
                field: 0,
            }),
            field: 1,
        }
    }

    fn read(base: usize) -> Rvalue<'static> {
        Rvalue::Use(Operand::Copy(nested_field(base)))
    }

    fn goto(target: usize) -> Terminator<'static> {
        Terminator::Goto {
            target: BlockId(target),
        }
    }

    fn call(target: usize) -> Terminator<'static> {
        Terminator::Call {
            has_trace: false,
            argument_layout: None,
            callee: Operand::copy_local(Local(1)),
            args: Vec::new(),
            ntypeargs: 0,
            destination: Place::Local(Local(0)),
            target: BlockId(target),
            unwind: None,
        }
    }

    fn value_of<'a>(
        body: &'a MirFunctionBody<'static>,
        block: usize,
        statement: usize,
    ) -> &'a Rvalue<'static> {
        match &body.blocks[block].statements[statement].kind {
            StatementKind::Assign { value, .. } => value,
            other => panic!("not an assignment: {other:?}"),
        }
    }

    fn copies(
        body: &MirFunctionBody<'static>,
        block: usize,
        statement: usize,
        source: usize,
    ) -> bool {
        matches!(
            value_of(body, block, statement),
            Rvalue::Use(Operand::Copy(Place::Local(local))) if local.0 == source
        )
    }

    fn computes(body: &MirFunctionBody<'static>, block: usize, statement: usize) -> bool {
        !matches!(
            value_of(body, block, statement),
            Rvalue::Use(Operand::Copy(Place::Local(_)))
        )
    }

    #[test]
    fn a_repeated_computation_reads_the_first_result() {
        let mut body = body(
            vec![block(
                0,
                vec![
                    set(3, divide(1, 2)),
                    set(4, divide(1, 2)),
                    set(5, divide(1, 2)),
                ],
                Terminator::Return,
            )],
            6,
        );
        reuse_available_values(&mut body);
        assert!(computes(&body, 0, 0));
        assert!(copies(&body, 0, 1, 3));
        assert!(copies(&body, 0, 2, 3));
    }

    #[test]
    fn one_cheap_reuse_does_not_pay_for_a_slot() {
        let mut body = body(
            vec![block(
                0,
                vec![set(3, divide(1, 2)), set(4, divide(1, 2)), set(0, copy(4))],
                Terminator::Return,
            )],
            5,
        );
        reuse_available_values(&mut body);
        assert!(computes(&body, 0, 1));
    }

    #[test]
    fn a_declined_reuse_left_without_readers_becomes_a_copy() {
        let mut body = body(
            vec![block(
                0,
                vec![
                    set(3, divide(1, 2)),
                    set(9, int(0)),
                    set(4, divide(3, 2)),
                    // Not worth a slot for `_3` on its own.
                    set(5, divide(1, 2)),
                    // Both read `_4` instead, which leaves `_5` unread.
                    set(6, divide(5, 2)),
                    set(7, divide(5, 2)),
                ],
                Terminator::Return,
            )],
            10,
        );
        reuse_available_values(&mut body);
        assert!(copies(&body, 0, 4, 4));
        assert!(copies(&body, 0, 5, 4));
        // A division kept only to be discarded would still have to run.
        assert!(copies(&body, 0, 3, 3));
    }

    #[test]
    fn a_write_to_an_operand_ends_the_reuse() {
        let mut body = body(
            vec![block(
                0,
                vec![
                    set(3, divide(1, 2)),
                    set(2, int(7)),
                    set(4, divide(1, 2)),
                    set(5, divide(1, 2)),
                    set(6, divide(1, 2)),
                ],
                Terminator::Return,
            )],
            7,
        );
        reuse_available_values(&mut body);
        // `_2` changed, so the division after the write is a new value, and
        // may trap where the first did not.
        assert!(computes(&body, 0, 2));
        assert!(copies(&body, 0, 3, 4));
        assert!(copies(&body, 0, 4, 4));
    }

    #[test]
    fn reuse_sees_through_copies_of_the_operands() {
        let mut body = body(
            vec![block(
                0,
                vec![
                    set(3, copy(1)),
                    set(4, divide(3, 2)),
                    set(5, copy(1)),
                    set(6, divide(5, 2)),
                    set(7, copy(1)),
                    set(8, divide(7, 2)),
                ],
                Terminator::Return,
            )],
            9,
        );
        reuse_available_values(&mut body);
        assert!(copies(&body, 0, 3, 4));
        assert!(copies(&body, 0, 5, 4));
    }

    #[test]
    fn a_call_ends_the_reuse_of_a_heap_read_but_not_of_arithmetic() {
        let mut body = body(
            vec![
                block(0, vec![set(3, read(1)), set(4, divide(1, 2))], call(1)),
                block(
                    1,
                    vec![
                        set(5, read(1)),
                        set(6, read(1)),
                        set(7, read(1)),
                        set(8, divide(1, 2)),
                        set(9, divide(1, 2)),
                        set(10, divide(1, 2)),
                        set(11, divide(1, 2)),
                    ],
                    Terminator::Return,
                ),
            ],
            12,
        );
        reuse_available_values(&mut body);
        // The callee, or a task that ran while it did, may have stored to the
        // field: the first read after the call is a real read.
        assert!(computes(&body, 1, 0));
        assert!(copies(&body, 1, 1, 5));
        assert!(copies(&body, 1, 2, 5));
        // Locals are this frame's own; the call cannot have changed them.
        assert!(copies(&body, 1, 3, 4));
        assert!(copies(&body, 1, 4, 4));
        assert!(copies(&body, 1, 5, 4));
        assert!(copies(&body, 1, 6, 4));
    }

    #[test]
    fn one_reuse_pays_when_the_holder_stays_on_the_stack() {
        let mut body = body(
            vec![block(
                0,
                vec![set(3, divide(1, 2)), set(4, copy(3)), set(5, divide(1, 2))],
                Terminator::Return,
            )],
            6,
        );
        reuse_available_values(&mut body);
        assert!(copies(&body, 0, 2, 3));
    }

    #[test]
    fn a_write_on_the_back_edge_ends_the_reuse_in_the_loop() {
        let mut body = body(
            vec![
                block(0, vec![set(3, divide(1, 2))], goto(1)),
                block(
                    1,
                    vec![
                        set(4, divide(1, 2)),
                        set(5, divide(1, 2)),
                        set(6, divide(1, 2)),
                    ],
                    Terminator::Branch {
                        condition: Operand::copy_local(Local(1)),
                        then_block: BlockId(2),
                        else_block: BlockId(3),
                    },
                ),
                block(2, vec![set(2, int(0))], goto(1)),
                block(3, Vec::new(), Terminator::Return),
            ],
            7,
        );
        reuse_available_values(&mut body);
        // The second trip divides by the new `_2`: the header's first
        // division is not the one bb0 ran.
        assert!(computes(&body, 1, 0));
        assert!(copies(&body, 1, 1, 4));
        assert!(copies(&body, 1, 2, 4));
    }

    #[test]
    fn a_heap_read_is_not_carried_around_a_loop() {
        // bb0 reads the field, the header bb1 reads it again on every trip.
        let reads = || (3..9).map(|local| set(local, read(1))).collect::<Vec<_>>();
        let mut body = body(
            vec![
                block(0, vec![set(2, read(1))], goto(1)),
                block(
                    1,
                    reads(),
                    Terminator::Branch {
                        condition: Operand::copy_local(Local(9)),
                        then_block: BlockId(2),
                        else_block: BlockId(3),
                    },
                ),
                block(2, Vec::new(), goto(1)),
                block(3, Vec::new(), Terminator::Return),
            ],
            10,
        );
        reuse_available_values(&mut body);
        // A later trip has to see a write another task made meanwhile, so the
        // header reads once per trip and reuses that.
        assert!(computes(&body, 1, 0));
        assert!(copies(&body, 1, 1, 3));
    }

    #[test]
    fn a_store_to_the_field_through_any_object_ends_the_reuse() {
        for (stored_field, blocked) in [(1, true), (2, false)] {
            let store = assign(
                Place::Field {
                    base: Box::new(Place::Local(Local(2))),
                    field: stored_field,
                },
                int(0),
            );
            let mut body = body(
                vec![block(
                    0,
                    vec![
                        set(3, read(1)),
                        store,
                        set(4, read(1)),
                        set(5, read(1)),
                        set(6, read(1)),
                    ],
                    Terminator::Return,
                )],
                7,
            );
            reuse_available_values(&mut body);
            // `_2` may be the object `_1.0` names.
            assert_eq!(computes(&body, 0, 2), blocked, "store to .{stored_field}");
            let holder = if blocked { 4 } else { 3 };
            assert!(copies(&body, 0, 3, holder));
            assert!(copies(&body, 0, 4, holder));
        }
    }

    #[test]
    fn a_value_computed_on_one_path_is_not_reused_at_the_join() {
        let mut body = body(
            vec![
                block(
                    0,
                    Vec::new(),
                    Terminator::Branch {
                        condition: Operand::copy_local(Local(1)),
                        then_block: BlockId(1),
                        else_block: BlockId(2),
                    },
                ),
                block(1, vec![set(3, divide(1, 2))], goto(3)),
                block(2, Vec::new(), goto(3)),
                block(
                    3,
                    vec![
                        set(4, divide(1, 2)),
                        set(5, divide(1, 2)),
                        set(6, divide(1, 2)),
                    ],
                    Terminator::Return,
                ),
            ],
            7,
        );
        reuse_available_values(&mut body);
        // Through bb2 the division has not run yet: its trap belongs here.
        assert!(computes(&body, 3, 0));
        assert!(copies(&body, 3, 1, 4));
        assert!(copies(&body, 3, 2, 4));
    }

    #[test]
    fn a_handler_starts_with_nothing_available() {
        let mut protected = block(0, vec![set(3, divide(1, 2))], goto(2));
        protected.unwind = Some(BlockId(1));
        let mut handler = block(
            1,
            vec![
                set(4, divide(1, 2)),
                set(5, divide(1, 2)),
                set(6, divide(1, 2)),
            ],
            Terminator::Return,
        );
        handler.landing = Some(Landing {
            error_local: Local(7),
            context_local: Local(8),
        });
        let mut body = body(
            vec![protected, handler, block(2, Vec::new(), Terminator::Return)],
            9,
        );
        reuse_available_values(&mut body);
        // The handler is entered when the protected division traps, before
        // `_3` is written.
        assert!(computes(&body, 1, 0));
    }

    fn negated_branch(uses_negation_again: bool, source_ty: RuntimeTy) -> MirFunctionBody<'static> {
        let not = Rvalue::UnaryOp {
            op: UnaryOp::Not,
            operand: Operand::copy_local(Local(1)),
        };
        let mut exit = Vec::new();
        if uses_negation_again {
            exit.push(set(0, copy(2)));
        }
        let mut body = body(
            vec![
                block(
                    0,
                    vec![set(2, not)],
                    Terminator::Branch {
                        condition: Operand::copy_local(Local(2)),
                        then_block: BlockId(1),
                        else_block: BlockId(2),
                    },
                ),
                block(1, exit, Terminator::Return),
                block(2, Vec::new(), Terminator::Return),
            ],
            3,
        );
        body.locals[1].ty = source_ty;
        body.locals[2].ty = RuntimeTy::Bool;
        body
    }

    #[test]
    fn a_branch_on_a_negation_branches_on_its_operand() {
        let mut body = negated_branch(false, RuntimeTy::Bool);
        fold_negated_branches(&mut body);
        assert!(body.blocks[0].statements.is_empty());
        assert!(matches!(
            &body.blocks[0].terminator,
            Some(Terminator::Branch {
                condition: Operand::Copy(Place::Local(Local(1))),
                then_block: BlockId(2),
                else_block: BlockId(1),
            })
        ));
    }

    #[test]
    fn a_negation_read_elsewhere_or_of_a_non_bool_keeps_its_branch() {
        for mut body in [
            negated_branch(true, RuntimeTy::Bool),
            negated_branch(false, RuntimeTy::int()),
        ] {
            fold_negated_branches(&mut body);
            assert_eq!(body.blocks[0].statements.len(), 1);
            assert!(matches!(
                &body.blocks[0].terminator,
                Some(Terminator::Branch {
                    then_block: BlockId(1),
                    ..
                })
            ));
        }
    }

    #[test]
    fn a_jump_to_a_bare_return_returns() {
        let mut body = body(
            vec![
                block(0, vec![set(0, int(1))], goto(1)),
                block(1, Vec::new(), Terminator::Return),
            ],
            1,
        );
        thread_returns(&mut body);
        assert!(matches!(
            body.blocks[0].terminator,
            Some(Terminator::Return)
        ));
    }

    #[test]
    fn a_return_block_with_work_or_another_handler_is_still_jumped_to() {
        let handler = || {
            let mut handler = block(2, Vec::new(), Terminator::Return);
            handler.landing = Some(Landing {
                error_local: Local(1),
                context_local: Local(2),
            });
            handler
        };
        let with_work = body(
            vec![
                block(0, Vec::new(), goto(1)),
                block(1, vec![set(0, int(1))], Terminator::Return),
            ],
            1,
        );
        let mut protected = block(0, vec![set(0, int(1))], goto(1));
        protected.unwind = Some(BlockId(2));
        let other_handler = body(
            vec![
                protected,
                block(1, Vec::new(), Terminator::Return),
                handler(),
            ],
            3,
        );
        for mut body in [with_work, other_handler] {
            thread_returns(&mut body);
            assert!(matches!(
                body.blocks[0].terminator,
                Some(Terminator::Goto { .. })
            ));
        }
    }
}
