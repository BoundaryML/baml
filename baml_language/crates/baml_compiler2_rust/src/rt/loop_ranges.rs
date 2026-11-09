//! Conservative bounds for counted loops. Only remove an i63 check when every
//! definition and every enclosing loop has a finite, checked bound. Unknown
//! control flow, aliases, recurrences and calls keep the ordinary error path.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Range(i128, i128);
impl Range {
    fn checked(low: i128, high: i128) -> Option<Self> {
        (low <= high
            && low >= i128::from(baml_type::Int63::MIN.get())
            && high <= i128::from(baml_type::Int63::MAX.get()))
        .then_some(Self(low, high))
    }
    fn join(self, other: Self) -> Self {
        Self(self.0.min(other.0), self.1.max(other.1))
    }
    fn binary(self, op: BinOp, rhs: Self) -> Option<Self> {
        match op {
            BinOp::Add => Self::checked(self.0 + rhs.0, self.1 + rhs.1),
            BinOp::Sub => Self::checked(self.0 - rhs.1, self.1 - rhs.0),
            BinOp::Mul => {
                let ends = [
                    self.0 * rhs.0,
                    self.0 * rhs.1,
                    self.1 * rhs.0,
                    self.1 * rhs.1,
                ];
                Self::checked(*ends.iter().min()?, *ends.iter().max()?)
            }
            _ => None,
        }
    }
}

fn local(operand: &Operand<'_>) -> Option<usize> {
    match operand {
        Operand::Copy(Place::Local(l)) | Operand::Move(Place::Local(l)) => Some(l.0),
        _ => None,
    }
}
fn integer(operand: &Operand<'_>) -> Option<i128> {
    match operand {
        Operand::Constant(Constant::Int(n)) => Some(i128::from(*n)),
        _ => None,
    }
}
fn operand(operand: &Operand<'_>, ranges: &[Option<Range>]) -> Option<Range> {
    integer(operand)
        .map(|n| Range(n, n))
        .or_else(|| ranges[local(operand)?])
}
fn value(value: &Rvalue<'_>, ranges: &[Option<Range>]) -> Option<Range> {
    match value {
        Rvalue::Use(o) => operand(o, ranges),
        Rvalue::BinaryOp { op, left, right } => {
            operand(left, ranges)?.binary(*op, operand(right, ranges)?)
        }
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum Definition<'a, 'db> {
    Value(usize, &'a Rvalue<'db>),
    Call(&'a Terminator<'db>),
}

fn definitions<'a, 'db>(body: &'a MirFunctionBody<'db>) -> Vec<Vec<Definition<'a, 'db>>> {
    let mut defs = vec![Vec::new(); body.locals.len()];
    for block in &body.blocks {
        for statement in &block.statements {
            if let StatementKind::Assign {
                destination: Place::Local(l),
                value,
            } = &statement.kind
            {
                defs[l.0].push(Definition::Value(block.id.0, value));
            }
        }
        if let Some(
            term @ (Terminator::Call {
                destination: Place::Local(l),
                ..
            }
            | Terminator::VirtualCall {
                destination: Place::Local(l),
                ..
            }),
        ) = &block.terminator
        {
            defs[l.0].push(Definition::Call(term));
        }
    }
    defs
}

fn continue_count(stmts: &[Stmt], label: usize) -> usize {
    stmts
        .iter()
        .map(|stmt| match stmt {
            Stmt::Continue(l) => usize::from(*l == label),
            Stmt::Loop { body, .. } | Stmt::Block { body, .. } => continue_count(body, label),
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => continue_count(then_branch, label) + continue_count(else_branch, label),
            _ => 0,
        })
        .sum()
}

fn leaves(stmts: &[Stmt], out: &mut BTreeSet<usize>) {
    for stmt in stmts {
        match stmt {
            Stmt::Leaf(b) => {
                out.insert(*b);
            }
            Stmt::Loop { body, .. } | Stmt::Block { body, .. } => leaves(body, out),
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                leaves(then_branch, out);
                leaves(else_branch, out);
            }
            Stmt::Switch {
                arms, otherwise, ..
            } => {
                for (_, arm) in arms {
                    leaves(arm, out);
                }
                leaves(otherwise, out);
            }
            _ => {}
        }
    }
}

// A head test, one positive increment at the tail, and exactly one back edge.
// Breaks/returns/errors can shorten this loop; none can increase its bound.
fn counted_loop(
    body: &MirFunctionBody<'_>,
    defs: &[Vec<Definition<'_, '_>>],
    arity: usize,
    label: usize,
    stmts: &[Stmt],
) -> Option<(usize, Range, u128)> {
    let [
        Stmt::Leaf(header),
        Stmt::If {
            block,
            then_branch,
            else_branch,
        },
    ] = stmts
    else {
        return None;
    };
    if header != block
        || else_branch.as_slice() != [Stmt::BreakLoop(label)]
        || then_branch.last() != Some(&Stmt::Continue(label))
        || continue_count(then_branch, label) != 1
    {
        return None;
    }
    let Stmt::Leaf(tail) = then_branch.get(then_branch.len().checked_sub(2)?)? else {
        return None;
    };
    let block = &body.blocks[*header];
    let Terminator::Branch { condition, .. } = block.terminator.as_ref()? else {
        return None;
    };
    let condition = local(condition)?;
    let (comparison_at, comparison) =
        block
            .statements
            .iter()
            .enumerate()
            .rev()
            .find_map(|(at, s)| match &s.kind {
                StatementKind::Assign {
                    destination: Place::Local(l),
                    value,
                } if l.0 == condition => Some((at, value)),
                _ => None,
            })?;
    let Rvalue::BinaryOp {
        op: BinOp::Lt,
        left,
        right,
    } = comparison
    else {
        return None;
    };
    let bound = integer(right)?;
    let mut induction = local(left)?;
    // MIR copies the named induction variable into a temporary for its test.
    for statement in block.statements[..comparison_at].iter().rev() {
        if let StatementKind::Assign {
            destination: Place::Local(l),
            value,
        } = &statement.kind
            && l.0 == induction
        {
            let Rvalue::Use(o) = value else { return None };
            induction = local(o)?;
        }
    }
    if induction <= arity || body.locals[induction].is_captured {
        return None;
    }
    let mut inside = BTreeSet::new();
    leaves(stmts, &mut inside);
    let (mut initial, mut step) = (None, None);
    for def in &defs[induction] {
        match def {
            Definition::Value(at, Rvalue::Use(o))
                if integer(o).is_some() && !inside.contains(at) =>
            {
                let n = integer(o)?;
                if initial.is_some_and(|old| old != n) {
                    return None;
                }
                initial = Some(n);
            }
            Definition::Value(
                at,
                Rvalue::BinaryOp {
                    op: BinOp::Add,
                    left,
                    right,
                },
            ) if *at == *tail && local(left) == Some(induction) && step.is_none() => {
                let n = integer(right)?;
                if n <= 0 {
                    return None;
                }
                step = Some(n);
            }
            _ => return None,
        }
    }
    let (initial, step) = (initial?, step?);
    let iterations = ((bound - initial).max(0) + step - 1) / step;
    let end = initial + iterations * step;
    Some((
        induction,
        Range::checked(initial, end)?,
        u128::try_from(iterations).ok()?,
    ))
}

fn budgets(
    body: &MirFunctionBody<'_>,
    defs: &[Vec<Definition<'_, '_>>],
    arity: usize,
    stmts: &[Stmt],
    budget: Option<u128>,
    visits: &mut [Option<u128>],
    ranges: &mut [Option<Range>],
) {
    for stmt in stmts {
        match stmt {
            Stmt::Leaf(b) => visits[*b] = budget,
            Stmt::Block { body: stmts, .. } => {
                budgets(body, defs, arity, stmts, budget, visits, ranges)
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                budgets(body, defs, arity, then_branch, budget, visits, ranges);
                budgets(body, defs, arity, else_branch, budget, visits, ranges);
            }
            Stmt::Loop { label, body: stmts } => {
                let count = counted_loop(body, defs, arity, *label, stmts).map(|(l, range, n)| {
                    ranges[l] = Some(range);
                    n + 1 // Also bound the final head test, conservatively for every block.
                });
                let nested = budget.and_then(|n| n.checked_mul(count?));
                budgets(body, defs, arity, stmts, nested, visits, ranges);
            }
            _ => {}
        }
    }
}

// Identity leaves are common loop helpers. Only their successful return value
// is summarized: their stack guard, telemetry and errors are still emitted.
fn identity_call(
    emitter: &Emitter<'_, '_>,
    term: &Terminator<'_>,
    ranges: &[Option<Range>],
) -> Option<Range> {
    let Terminator::Call {
        callee: Operand::Constant(Constant::Function(f)),
        args,
        ntypeargs: 0,
        argument_layout,
        ..
    } = term
    else {
        return None;
    };
    let (c, info) = emitter.plan.index.function(emitter.db, *f)?;
    let MirFunctionKind::Bytecode(body) = &emitter.all.get(&c)?.kind else {
        return None;
    };
    let mut returned = None;
    let mut block = body.entry;
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(block.0) {
            return None;
        }
        let current = &body.blocks[block.0];
        for s in &current.statements {
            match &s.kind {
                StatementKind::Assign {
                    destination: Place::Local(l),
                    value: Rvalue::Use(o),
                } if l.0 == 0 && returned.is_none() => returned = local(o),
                StatementKind::Nop => {}
                _ => return None,
            }
        }
        match current.terminator.as_ref()? {
            Terminator::Goto { target } => block = *target,
            Terminator::Return => break,
            _ => return None,
        }
    }
    let parameter = returned?.checked_sub(1)?;
    if parameter >= emitter.all[&c].arity {
        return None;
    }
    let argument = if let Some(layout) = argument_layout {
        layout
            .map_to(&info.argument_layout())
            .ok()?
            .get(parameter)
            .copied()
            .flatten()?
    } else {
        parameter
    };
    operand(args.get(argument)?, ranges)
}

fn local_range(
    emitter: &Emitter<'_, '_>,
    l: usize,
    defs: &[Definition<'_, '_>],
    visits: &[Option<u128>],
    ranges: &[Option<Range>],
) -> Option<Range> {
    let (mut initial, mut ordinary) = (None::<Range>, None::<Range>);
    let (mut low, mut high, mut recurrence) = (0i128, 0i128, false);
    for def in defs {
        match def {
            Definition::Value(_, Rvalue::Use(o)) if integer(o).is_some() => {
                let n = integer(o)?;
                initial = Some(initial.map_or(Range(n, n), |old| old.join(Range(n, n))));
            }
            Definition::Value(_, Rvalue::Use(o)) if local(o) == Some(l) => {}
            Definition::Value(
                block,
                Rvalue::BinaryOp {
                    op: BinOp::Add,
                    left,
                    right,
                },
            ) if local(left) == Some(l) => {
                recurrence = true;
                let delta = operand(right, ranges)?;
                let n = i128::try_from(visits[*block]?).ok()?;
                low = low.checked_add(delta.0.min(0).checked_mul(n)?)?;
                high = high.checked_add(delta.1.max(0).checked_mul(n)?)?;
            }
            _ => {
                let range = match def {
                    Definition::Value(_, v) => value(v, ranges)?,
                    Definition::Call(term) => identity_call(emitter, term, ranges)?,
                };
                ordinary = Some(ordinary.map_or(range, |old| old.join(range)));
            }
        }
    }
    if recurrence {
        if ordinary.is_some() {
            return None;
        }
        let start = initial?;
        Range::checked(start.0.checked_add(low)?, start.1.checked_add(high)?)
    } else {
        match (initial, ordinary) {
            (Some(a), Some(b)) => Some(a.join(b)),
            (a, b) => a.or(b),
        }
    }
}

pub(super) fn analyze(emitter: &Emitter<'_, '_>, stmts: &[Stmt]) -> BTreeSet<(usize, usize)> {
    let body = emitter.body;
    let defs = definitions(body);
    let mut ranges = vec![None; body.locals.len()];
    for range in &mut ranges[1..=emitter.arity] {
        *range = Some(Range(
            i128::from(baml_type::Int63::MIN.get()),
            i128::from(baml_type::Int63::MAX.get()),
        ));
    }
    let mut visits = vec![None; body.blocks.len()];
    budgets(
        body,
        &defs,
        emitter.arity,
        stmts,
        Some(1),
        &mut visits,
        &mut ranges,
    );
    loop {
        let mut changed = false;
        for l in 0..ranges.len() {
            if ranges[l].is_none()
                && !body.locals[l].is_captured
                && let Some(range) = local_range(emitter, l, &defs[l], &visits, &ranges)
            {
                ranges[l] = Some(range);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    body.blocks
        .iter()
        .flat_map(|block| {
            block
                .statements
                .iter()
                .enumerate()
                .filter_map(|(s, statement)| {
                    let StatementKind::Assign {
                        value:
                            v @ Rvalue::BinaryOp {
                                op: BinOp::Add | BinOp::Sub | BinOp::Mul,
                                ..
                            },
                        ..
                    } = &statement.kind
                    else {
                        return None;
                    };
                    value(v, &ranges).map(|_| (block.id.0, s))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

pub(super) struct Feature;
impl registry::Feature for Feature {
    fn value(
        &self,
        emitter: &Emitter<'_, '_>,
        value: &Rvalue<'_>,
        expected: Repr,
    ) -> Option<Gen<(String, Repr)>> {
        if expected != Repr::Int
            || !emitter
                .proven_arithmetic
                .contains(&(emitter.block, emitter.statement))
        {
            return None;
        }
        let Rvalue::BinaryOp { op, left, right } = value else {
            return None;
        };
        let method = match op {
            BinOp::Add => "wrapping_add",
            BinOp::Sub => "wrapping_sub",
            BinOp::Mul => "wrapping_mul",
            _ => return None,
        };
        Some((|| {
            let (left, lt) = emitter.operand(left)?;
            let (right, rt) = emitter.operand(right)?;
            // MIR can keep a union of integer literals in a tagged temporary.
            // The proof bounds the integer value, independent of its storage.
            let left = emitter.coerce((left, lt), Repr::Int)?;
            let right = emitter.coerce((right, rt), Repr::Int)?;
            Ok((format!("({left}).{method}({right})"), Repr::Int))
        })())
    }
}
