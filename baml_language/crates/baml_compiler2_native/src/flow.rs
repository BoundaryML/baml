//! What the shape of a function's loops says about it: where a loop ends an
//! iteration, and which array accesses a counted loop proves in range.
//!
//! A counted loop is one whose test is `i < bound`, where `i` starts at a
//! constant that is not negative and its only other assignment is one
//! `i = i + step` per iteration. Such an `i` is never negative, anywhere.
//! When the bound is an array's length less amounts that are not negative,
//! and nothing in the loop can shorten the array, `array[i + d]` between the
//! test and the step names an element for every `d` from zero to the
//! constant part of what was subtracted: no counting from the end, no check.

use std::collections::{HashMap, HashSet};

use baml_compiler2_mir::{
    BinOp, Constant, Local, MirFunctionBody, Operand, Place, Rvalue, StatementKind, Terminator,
};

/// The largest step of a counted loop. With it, and arrays far shorter than
/// the largest int, `i + step` cannot wrap.
const MAX_STEP: i64 = 1 << 31;
/// The largest constant a counted loop may count to, for the same reason.
const MAX_BOUND: i64 = 1 << 40;

/// One loop: the blocks that can reach a jump back to `header` without
/// leaving through it.
pub(crate) struct Loop {
    pub(crate) header: usize,
    /// The blocks that jump back to the header.
    pub(crate) latches: Vec<usize>,
    pub(crate) blocks: Vec<bool>,
}

pub(crate) struct Flow {
    successors: Vec<Vec<usize>>,
    predecessors: Vec<Vec<usize>>,
    pub(crate) loops: Vec<Loop>,
}

impl Flow {
    pub(crate) fn of(body: &MirFunctionBody<'_>) -> Self {
        let count = body.blocks.len();
        let successors: Vec<Vec<usize>> = body
            .blocks
            .iter()
            .map(|block| {
                let mut successors: Vec<usize> = block
                    .terminator
                    .as_ref()
                    .map(|terminator| terminator.successors().iter().map(|id| id.0).collect())
                    .unwrap_or_default();
                // A throw anywhere in the block goes to its handler.
                if let Some(handler) = block.unwind
                    && !successors.contains(&handler.0)
                {
                    successors.push(handler.0);
                }
                successors
            })
            .collect();
        let mut predecessors = vec![Vec::new(); count];
        for (block, targets) in successors.iter().enumerate() {
            for &target in targets {
                predecessors[target].push(block);
            }
        }
        let mut flow = Self {
            successors,
            predecessors,
            loops: Vec::new(),
        };
        flow.find_loops(body.entry.0);
        flow
    }

    /// The blocks reached from `from` along `edges`, entering only blocks
    /// `open` allows. The starting blocks are included when open.
    fn reach(
        edges: &[Vec<usize>],
        from: impl IntoIterator<Item = usize>,
        open: impl Fn(usize) -> bool,
    ) -> Vec<bool> {
        let mut reached = vec![false; edges.len()];
        let mut pending: Vec<usize> = from.into_iter().collect();
        while let Some(block) = pending.pop() {
            if reached[block] || !open(block) {
                continue;
            }
            reached[block] = true;
            pending.extend(edges[block].iter().copied());
        }
        reached
    }

    fn find_loops(&mut self, entry: usize) {
        enum Visit {
            Enter(usize),
            Leave(usize),
        }
        let count = self.successors.len();
        let mut entered = vec![false; count];
        let mut open = vec![false; count];
        let mut latches: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut pending = vec![Visit::Enter(entry)];
        while let Some(visit) = pending.pop() {
            match visit {
                Visit::Leave(block) => open[block] = false,
                Visit::Enter(block) if entered[block] => {}
                Visit::Enter(block) => {
                    entered[block] = true;
                    open[block] = true;
                    pending.push(Visit::Leave(block));
                    for &successor in &self.successors[block] {
                        if open[successor] {
                            latches.entry(successor).or_default().push(block);
                        } else if !entered[successor] {
                            pending.push(Visit::Enter(successor));
                        }
                    }
                }
            }
        }
        let mut headers: Vec<_> = latches.into_iter().collect();
        headers.sort_unstable();
        for (header, latches) in headers {
            let mut blocks = Self::reach(&self.predecessors, latches.iter().copied(), |block| {
                block != header
            });
            blocks[header] = true;
            self.loops.push(Loop {
                header,
                latches,
                blocks,
            });
        }
    }
}

/// Whether control can come back from this terminator having run other
/// code: a call of any kind.
pub(crate) fn runs_other_code(terminator: &Terminator<'_>) -> bool {
    !matches!(
        terminator,
        Terminator::Goto { .. }
            | Terminator::Branch { .. }
            | Terminator::Switch { .. }
            | Terminator::Return
            | Terminator::Unreachable
            | Terminator::ShortCircuit { .. }
    )
}

fn operand_local(operand: &Operand<'_>) -> Option<Local> {
    match operand {
        Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => Some(*local),
        _ => None,
    }
}

fn operand_int(operand: &Operand<'_>) -> Option<i64> {
    match operand {
        Operand::Constant(Constant::Int(value)) => Some(*value),
        _ => None,
    }
}

/// The local a terminator assigns, if it assigns one.
pub(crate) fn terminator_destination(terminator: &Terminator<'_>) -> Option<Local> {
    let place = match terminator {
        Terminator::NarrowBind { destination, .. } => return Some(*destination),
        Terminator::Call { destination, .. }
        | Terminator::VirtualCall { destination, .. }
        | Terminator::SysOp { destination, .. }
        | Terminator::Await { destination, .. }
        | Terminator::AwaitAny { destination, .. }
        | Terminator::ShortCircuit { destination, .. } => destination,
        Terminator::Spawn { future, .. } => future,
        _ => return None,
    };
    match place {
        Place::Local(local) => Some(*local),
        _ => None,
    }
}

/// Where a local is assigned.
#[derive(Clone, Copy)]
enum Def {
    Statement { block: usize, index: usize },
    Other,
}

struct Defs<'a, 'db> {
    body: &'a MirFunctionBody<'db>,
    of: HashMap<Local, Vec<Def>>,
}

impl<'a, 'db> Defs<'a, 'db> {
    fn of(body: &'a MirFunctionBody<'db>, arity: usize) -> Self {
        let mut of: HashMap<Local, Vec<Def>> = HashMap::new();
        for parameter in 1..=arity {
            of.entry(Local(parameter)).or_default().push(Def::Other);
        }
        for (block, contents) in body.blocks.iter().enumerate() {
            for (index, statement) in contents.statements.iter().enumerate() {
                if let StatementKind::Assign {
                    destination: Place::Local(local),
                    ..
                } = &statement.kind
                {
                    of.entry(*local)
                        .or_default()
                        .push(Def::Statement { block, index });
                }
            }
            if let Some(local) = contents
                .terminator
                .as_ref()
                .and_then(terminator_destination)
            {
                of.entry(local).or_default().push(Def::Other);
            }
        }
        Self { body, of }
    }

    fn value(&self, block: usize, index: usize) -> Option<&'a Rvalue<'db>> {
        match &self.body.blocks[block].statements[index].kind {
            StatementKind::Assign { value, .. } => Some(value),
            _ => None,
        }
    }

    /// The one assignment of a local assigned once, by a statement.
    fn single(&self, local: Local) -> Option<(usize, &'a Rvalue<'db>)> {
        match self.of.get(&local).map(Vec::as_slice) {
            Some([Def::Statement { block, index }]) => Some((*block, self.value(*block, *index)?)),
            _ => None,
        }
    }

    /// The local behind a chain of once-assigned copies.
    fn behind_copies(&self, mut local: Local, allowed: impl Fn(usize) -> bool) -> Local {
        while let Some((block, Rvalue::Use(operand))) = self.single(local)
            && allowed(block)
            && let Some(source) = operand_local(operand)
        {
            local = source;
        }
        local
    }
}

/// What a counted loop's test compares its counter with.
enum Bound {
    Constant,
    /// The length of the array in a local, less at least `slack`, computed
    /// in `blocks`.
    Length {
        array: Local,
        slack: i64,
        blocks: Vec<usize>,
    },
}

/// A counted loop.
struct Counted {
    counter: Local,
    bound: Bound,
    /// The block from whose start the test is known to hold.
    start: usize,
    /// The block an iteration passes to be tested again.
    gate: usize,
    /// The blocks from the header to a test at the top of the loop.
    head: Vec<usize>,
    /// Where the counter steps.
    step: (usize, usize),
}

/// The array accesses proven to name an element, as (block, array local,
/// index local) the way the access is written.
#[derive(Default)]
pub(crate) struct ProvenIndexes {
    proven: HashSet<(usize, Local, Local)>,
}

impl ProvenIndexes {
    pub(crate) fn holds(&self, block: usize, array: Local, index: Local) -> bool {
        self.proven.contains(&(block, array, index))
    }

    pub(crate) fn of(body: &MirFunctionBody<'_>, arity: usize, flow: &Flow) -> Self {
        let defs = Defs::of(body, arity);
        // A loop's bound can subtract an outer loop's counter, so loops are
        // recognised until no more are.
        let mut counters: HashSet<Local> = HashSet::new();
        loop {
            let known = counters.len();
            let found: Vec<Local> = flow
                .loops
                .iter()
                .filter_map(|each| count(body, flow, &defs, each, &counters))
                .map(|counted| counted.counter)
                .collect();
            counters.extend(found);
            if counters.len() == known {
                break;
            }
        }

        let mut proven = Self::default();
        for (each, counted) in flow
            .loops
            .iter()
            .filter_map(|each| Some((each, count(body, flow, &defs, each, &counters)?)))
        {
            proven.prove(body, flow, &defs, each, &counted);
        }
        proven
    }

    fn prove(
        &mut self,
        body: &MirFunctionBody<'_>,
        flow: &Flow,
        defs: &Defs<'_, '_>,
        each: &Loop,
        counted: &Counted,
    ) {
        let Bound::Length {
            array,
            slack,
            blocks,
        } = &counted.bound
        else {
            return;
        };
        // Between computing the bound and any iteration, nothing may
        // shorten the array or put another in its local.
        let to_header = Flow::reach(&flow.predecessors, [each.header], |_| true);
        let mut guarded = each.blocks.clone();
        for &block in blocks {
            let after = Flow::reach(&flow.successors, [block], |_| true);
            for (index, inside) in guarded.iter_mut().enumerate() {
                *inside |= after[index] && to_header[index];
            }
        }
        let quiet = guarded.iter().enumerate().all(|(block, inside)| {
            !inside
                || body.blocks[block]
                    .terminator
                    .as_ref()
                    .is_none_or(|terminator| !runs_other_code(terminator))
        });
        let array_kept = defs.of.get(array).is_none_or(|all| {
            all.iter().all(|def| match def {
                Def::Statement { block, .. } => !guarded[*block],
                Def::Other => true,
            })
        });
        if !quiet || !array_kept {
            return;
        }

        // The test holds from its loop edge up to the step: in every block
        // reached without stepping, and in the step's block before it.
        let inside = |block: usize| each.blocks[block] && block != counted.gate;
        let mut fresh = Flow::reach(
            &flow.successors,
            flow.successors[counted.start].iter().copied(),
            |block| inside(block) && !counted.head.contains(&block),
        );
        fresh[counted.start] = true;
        let (step_block, step_index) = counted.step;
        let stepped = Flow::reach(
            &flow.successors,
            flow.successors[step_block].iter().copied(),
            inside,
        );
        for (block, contents) in body.blocks.iter().enumerate() {
            if !fresh[block] || stepped[block] {
                continue;
            }
            let before_step = if block == step_block {
                step_index
            } else {
                contents.statements.len()
            };
            // Once-assigned temporaries holding the counter plus a constant
            // within the slack, and ones holding the array.
            let mut offsets: HashMap<Local, i64> = HashMap::new();
            let mut arrays: HashSet<Local> = HashSet::from([*array]);
            if block != step_block {
                offsets.insert(counted.counter, 0);
            }
            for (index, statement) in contents.statements.iter().enumerate() {
                let StatementKind::Assign {
                    destination: Place::Local(temporary),
                    value,
                } = &statement.kind
                else {
                    continue;
                };
                if defs.single(*temporary).is_none() {
                    continue;
                }
                let read = |local: Local| {
                    if local == counted.counter {
                        (index < before_step).then_some(0)
                    } else {
                        offsets.get(&local).copied()
                    }
                };
                let found = match value {
                    Rvalue::Use(operand) => operand_local(operand).and_then(read),
                    Rvalue::BinaryOp {
                        op: BinOp::Add,
                        left,
                        right,
                    } => operand_local(left)
                        .and_then(read)
                        .zip(operand_int(right))
                        .filter(|(_, more)| *more >= 0)
                        .and_then(|(offset, more)| offset.checked_add(more))
                        .filter(|sum| sum <= slack),
                    _ => None,
                };
                if let Some(offset) = found {
                    offsets.insert(*temporary, offset);
                } else if let Rvalue::Use(operand) = value
                    && operand_local(operand).is_some_and(|source| arrays.contains(&source))
                {
                    arrays.insert(*temporary);
                }
            }
            for &array in &arrays {
                for &index in offsets.keys() {
                    self.proven.insert((block, array, index));
                }
            }
        }
    }
}

/// A test `counter < bound` that decides whether control enters `target`.
struct Test<'a, 'db> {
    counter: Local,
    bound: &'a Operand<'db>,
    other: usize,
}

/// Read the terminator of `block` as a test guarding `target`.
fn test_before<'a, 'db>(
    body: &'a MirFunctionBody<'db>,
    flow: &Flow,
    defs: &Defs<'a, 'db>,
    block: usize,
    target: usize,
) -> Option<Test<'a, 'db>> {
    let Terminator::Branch {
        condition,
        then_block,
        else_block,
    } = body.blocks[block].terminator.as_ref()?
    else {
        return None;
    };
    if then_block.0 != target || else_block.0 == target {
        return None;
    }
    // The comparison and the copies of the counter it reads are in the
    // block, or in blocks that only ever jump straight to it.
    let mut chain = vec![block];
    while let &[before] = flow.predecessors[*chain.last()?].as_slice()
        && matches!(
            body.blocks[before].terminator,
            Some(Terminator::Goto { .. })
        )
        && !chain.contains(&before)
    {
        chain.push(before);
    }
    let (
        compared_in,
        Rvalue::BinaryOp {
            op: BinOp::Lt,
            left,
            right,
        },
    ) = defs.single(operand_local(condition)?)?
    else {
        return None;
    };
    let mut furthest = chain.iter().position(|&each| each == compared_in)?;
    let mut counter = operand_local(left)?;
    while let Some((copied_in, Rvalue::Use(operand))) = defs.single(counter)
        && let Some(at) = chain.iter().position(|&each| each == copied_in)
        && let Some(source) = operand_local(operand)
    {
        furthest = furthest.max(at);
        counter = source;
    }
    // The counter does not change between those copies and the test.
    let unchanged = defs.of.get(&counter)?.iter().all(|def| match def {
        Def::Statement { block, .. } => !chain[..=furthest].contains(block),
        Def::Other => false,
    });
    unchanged.then_some(Test {
        counter,
        bound: right,
        other: else_block.0,
    })
}

fn same_bound(left: &Operand<'_>, right: &Operand<'_>) -> bool {
    match (operand_local(left), operand_local(right)) {
        (Some(left), Some(right)) => left == right,
        (None, None) => operand_int(left).is_some() && operand_int(left) == operand_int(right),
        _ => false,
    }
}

/// Recognise `each` as a counted loop, given the counters known so far.
fn count(
    body: &MirFunctionBody<'_>,
    flow: &Flow,
    defs: &Defs<'_, '_>,
    each: &Loop,
    counters: &HashSet<Local>,
) -> Option<Counted> {
    // Only the header is entered from outside.
    let one_entry = each.blocks.iter().enumerate().all(|(block, inside)| {
        !inside
            || block == each.header
            || flow.predecessors[block]
                .iter()
                .all(|&from| each.blocks[from])
    });
    if !one_entry {
        return None;
    }

    // The test is at the top, reached from the header by plain jumps, or
    // the loop is turned over: tested before it is entered and again at the
    // end of each iteration.
    let mut head = Vec::new();
    let mut block = each.header;
    let top = loop {
        head.push(block);
        match body.blocks[block].terminator.as_ref()? {
            Terminator::Goto { target } if each.blocks[target.0] && !head.contains(&target.0) => {
                block = target.0;
            }
            Terminator::Branch { then_block, .. } => {
                break test_before(body, flow, defs, block, then_block.0)
                    .filter(|test| each.blocks[then_block.0] && !each.blocks[test.other])
                    .map(|test| (test, then_block.0, block));
            }
            _ => break None,
        }
    };
    let (test, start, gate) = match top {
        Some(top) => top,
        None => {
            head.clear();
            let mut tests = flow.predecessors[each.header]
                .iter()
                .map(|&from| Some((from, test_before(body, flow, defs, from, each.header)?)));
            let (_, first) = tests.next()??;
            for another in tests {
                let (from, another) = another?;
                if another.counter != first.counter
                    || !same_bound(another.bound, first.bound)
                    || (each.blocks[from] && each.blocks[another.other])
                {
                    return None;
                }
            }
            if flow.predecessors[each.header]
                .first()
                .is_some_and(|&from| each.blocks[from] && each.blocks[first.other])
            {
                return None;
            }
            (first, each.header, each.header)
        }
    };
    let counter = test.counter;

    // The counter: constants that are not negative outside the loop, one
    // step inside it, taken at most once per test.
    let mut step = None;
    for def in defs.of.get(&counter)? {
        let Def::Statement { block, index } = *def else {
            return None;
        };
        let value = defs.value(block, index)?;
        if each.blocks[block] {
            let Rvalue::BinaryOp {
                op: BinOp::Add,
                left,
                right,
            } = value
            else {
                return None;
            };
            let by = operand_int(right)?;
            if operand_local(left) != Some(counter)
                || !(1..=MAX_STEP).contains(&by)
                || step.is_some()
                || head.contains(&block)
            {
                return None;
            }
            step = Some((block, index));
        } else {
            let Rvalue::Use(start) = value else {
                return None;
            };
            if operand_int(start)? < 0 {
                return None;
            }
        }
    }
    let step = step?;
    let again = Flow::reach(
        &flow.successors,
        flow.successors[step.0].iter().copied(),
        |block| each.blocks[block] && block != gate,
    );
    if again[step.0] {
        return None;
    }

    let bound = match test.bound {
        Operand::Constant(Constant::Int(limit)) if *limit <= MAX_BOUND => Bound::Constant,
        bound => length_bound(defs, operand_local(bound)?, counters)?,
    };
    Some(Counted {
        counter,
        bound,
        start,
        gate,
        head,
        step,
    })
}

/// A bound that is an array's length less amounts that are not negative.
fn length_bound(defs: &Defs<'_, '_>, mut local: Local, counters: &HashSet<Local>) -> Option<Bound> {
    let mut slack = 0_i64;
    let mut blocks = Vec::new();
    loop {
        let (block, value) = defs.single(local)?;
        blocks.push(block);
        match value {
            Rvalue::Len(Place::Local(array)) => {
                return Some(Bound::Length {
                    array: *array,
                    slack,
                    blocks,
                });
            }
            Rvalue::Use(operand) => local = operand_local(operand)?,
            Rvalue::BinaryOp {
                op: BinOp::Sub,
                left,
                right,
            } => {
                match right {
                    Operand::Constant(Constant::Int(less)) if (0..=MAX_STEP).contains(less) => {
                        slack += less;
                    }
                    _ => {
                        let less = defs.behind_copies(operand_local(right)?, |_| true);
                        if !counters.contains(&less) {
                            return None;
                        }
                    }
                }
                local = operand_local(left)?;
            }
            _ => return None,
        }
    }
}
