//! Evaluation-site proofs for dead-code elimination. Facts are intersected at
//! joins and invalidated by writes/calls; a proof never classifies a whole local
//! as nontrapping across its other definitions or uses.

use std::collections::{HashMap, HashSet, VecDeque};

use baml_type::{Int63, Literal, RuntimeTy};

use crate::{
    BinOp, Constant, IndexKind, Local, MirFunctionBody, Operand, Place, Rvalue, StatementKind,
    Terminator, UnaryOp,
};

const MIN: i64 = Int63::MIN.get();
const MAX: i64 = Int63::MAX.get();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Range {
    lo: i64,
    hi: i64,
}

impl Range {
    const INT: Self = Self { lo: MIN, hi: MAX };
    const LENGTH: Self = Self { lo: 0, hi: MAX };

    fn exact(n: i64) -> Self {
        Self { lo: n, hi: n }
    }

    fn intersect(self, other: Self) -> Option<Self> {
        let result = Self {
            lo: self.lo.max(other.lo),
            hi: self.hi.min(other.hi),
        };
        (result.lo <= result.hi).then_some(result)
    }

    fn join(self, other: Self, widen: bool, domain: Self) -> Self {
        Self {
            lo: if widen && other.lo < self.lo {
                domain.lo
            } else {
                self.lo.min(other.lo)
            },
            hi: if widen && other.hi > self.hi {
                domain.hi
            } else {
                self.hi.max(other.hi)
            },
        }
    }

    fn arithmetic(self, op: BinOp, rhs: Self) -> Option<Self> {
        if matches!(op, BinOp::Div | BinOp::Mod)
            && ((rhs.lo <= 0 && rhs.hi >= 0) || (self.lo == MIN && rhs.lo <= -1 && rhs.hi >= -1))
        {
            return None;
        }
        if op == BinOp::Mod {
            return Some(Self::INT);
        }
        let mut lo = i128::MAX;
        let mut hi = i128::MIN;
        for a in [i128::from(self.lo), i128::from(self.hi)] {
            for b in [i128::from(rhs.lo), i128::from(rhs.hi)] {
                let n = match op {
                    BinOp::Add => a + b,
                    BinOp::Sub => a - b,
                    BinOp::Mul => a * b,
                    BinOp::Div => a / b,
                    _ => return None,
                };
                lo = lo.min(n);
                hi = hi.max(n);
            }
        }
        (lo >= i128::from(MIN) && hi <= i128::from(MAX)).then_some(Self {
            lo: i64::try_from(lo).ok()?,
            hi: i64::try_from(hi).ok()?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Number {
    Constant(i64),
    Local(Local),
    /// Current container length minus a nonnegative constant.
    Length(Local, i64),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ReadKey {
    Local(Local),
    Field(Box<Self>, usize),
    Index(Box<Self>, Number, IndexKind),
}

impl ReadKey {
    fn depends_on(&self, local: Local) -> bool {
        match self {
            Self::Local(root) => *root == local,
            Self::Field(base, _) => base.depends_on(local),
            Self::Index(base, index, _) => base.depends_on(local) || index.depends_on(local),
        }
    }
}

impl Number {
    fn depends_on(self, local: Local) -> bool {
        matches!(self, Self::Local(l) | Self::Length(l, _) if l == local)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Condition {
    op: BinOp,
    left: Number,
    right: Number,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Facts {
    // Copies refer to the same current value only until either slot is written.
    aliases: HashMap<Local, Local>,
    reads: HashMap<ReadKey, Local>,
    integers: HashMap<Local, Range>,
    lengths: HashMap<Local, Range>,
    length_values: HashMap<Local, (Local, i64)>,
    conditions: HashMap<Local, Condition>,
}

impl Facts {
    fn root(&self, local: Local) -> Local {
        self.aliases.get(&local).copied().unwrap_or(local)
    }

    fn read_key(&self, place: &Place, body: &MirFunctionBody<'_>) -> Option<ReadKey> {
        let key = match place {
            Place::Local(local) if !body.locals[local.0].is_captured => {
                return Some(ReadKey::Local(self.root(*local)));
            }
            Place::Field { base, field } => {
                ReadKey::Field(Box::new(self.read_key(base, body)?), *field)
            }
            Place::Index { base, index, kind } => {
                let mut index = self.number(&Operand::copy_local(*index), body)?;
                let range = self.range(index);
                if range.lo == range.hi {
                    index = Number::Constant(range.lo);
                }
                ReadKey::Index(Box::new(self.read_key(base, body)?), index, *kind)
            }
            _ => return None,
        };
        Some(
            self.reads
                .get(&key)
                .map_or(key.clone(), |local| ReadKey::Local(*local)),
        )
    }

    fn number(&self, operand: &Operand<'_>, body: &MirFunctionBody<'_>) -> Option<Number> {
        match operand {
            Operand::Constant(Constant::Int(n)) => Some(Number::Constant(*n)),
            Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local))
                if !body.locals[local.0].is_captured =>
            {
                let local = self.root(*local);
                if let Some((array, offset)) = self.length_values.get(&local) {
                    return Some(Number::Length(*array, *offset));
                }
                if self.integers.contains_key(&local)
                    || matches!(
                        body.locals[local.0].ty,
                        RuntimeTy::Int { .. } | RuntimeTy::Literal(Literal::Int(_), ..)
                    )
                {
                    Some(Number::Local(local))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn range(&self, number: Number) -> Range {
        match number {
            Number::Constant(n) => Range::exact(n),
            Number::Local(local) => self.integers.get(&local).copied().unwrap_or(Range::INT),
            Number::Length(local, offset) => {
                let range = self.lengths.get(&local).copied().unwrap_or(Range::LENGTH);
                Range {
                    lo: range.lo - offset,
                    hi: range.hi - offset,
                }
            }
        }
    }

    fn length_value(&self, value: &Rvalue<'_>, body: &MirFunctionBody<'_>) -> Option<(Local, i64)> {
        match value {
            Rvalue::Len(place) => match self.read_key(place, body)? {
                ReadKey::Local(local) => Some((local, 0)),
                _ => None,
            },
            Rvalue::Use(operand) => match self.number(operand, body)? {
                Number::Length(local, offset) => Some((local, offset)),
                _ => None,
            },
            Rvalue::BinaryOp {
                op: BinOp::Sub,
                left,
                right,
            } => {
                let Number::Length(local, offset) = self.number(left, body)? else {
                    return None;
                };
                let rhs = self.range(self.number(right, body)?);
                let offset = offset.checked_add(rhs.lo)?;
                (rhs.lo == rhs.hi && rhs.lo >= 0 && offset <= MAX).then_some((local, offset))
            }
            _ => None,
        }
    }

    fn integer_value(&self, value: &Rvalue<'_>, body: &MirFunctionBody<'_>) -> Option<Range> {
        match value {
            Rvalue::Use(operand) => Some(self.range(self.number(operand, body)?)),
            Rvalue::Len(_) => Some(Range::LENGTH),
            Rvalue::BinaryOp { op, left, right } => self
                .range(self.number(left, body)?)
                .arithmetic(*op, self.range(self.number(right, body)?)),
            Rvalue::UnaryOp {
                op: UnaryOp::Neg,
                operand,
            } => Range::exact(0).arithmetic(BinOp::Sub, self.range(self.number(operand, body)?)),
            _ => None,
        }
    }

    fn condition(&self, operand: &Operand<'_>) -> Option<Condition> {
        match operand {
            Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
                self.conditions.get(local).copied()
            }
            _ => None,
        }
    }

    fn forget(&mut self, local: Local) {
        self.aliases
            .retain(|dest, source| *dest != local && *source != local);
        self.reads
            .retain(|key, dest| *dest != local && !key.depends_on(local));
        self.integers.remove(&local);
        self.lengths.remove(&local);
        self.length_values
            .retain(|dest, (array, _)| *dest != local && *array != local);
        self.conditions.retain(|dest, condition| {
            *dest != local
                && !condition.left.depends_on(local)
                && !condition.right.depends_on(local)
        });
    }

    fn statement(&mut self, statement: &StatementKind<'_>, body: &MirFunctionBody<'_>) {
        match statement {
            StatementKind::Assign {
                destination: Place::Local(local),
                value,
            } => {
                let read = match value {
                    Rvalue::Use(Operand::Copy(place) | Operand::Move(place)) => {
                        self.read_key(place, body)
                    }
                    _ => None,
                };
                let integer = self.integer_value(value, body);
                let length_value = self.length_value(value, body);
                let condition = match value {
                    Rvalue::Use(operand) => self.condition(operand),
                    Rvalue::BinaryOp { op, left, right }
                        if matches!(
                            op,
                            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
                        ) =>
                    {
                        self.number(left, body).zip(self.number(right, body)).map(
                            |(left, right)| Condition {
                                op: *op,
                                left,
                                right,
                            },
                        )
                    }
                    Rvalue::UnaryOp {
                        op: UnaryOp::Not,
                        operand,
                    } => self.condition(operand).map(|condition| Condition {
                        op: invert(condition.op),
                        ..condition
                    }),
                    _ => None,
                };
                let length = match value {
                    Rvalue::Array(_, elements) => {
                        i64::try_from(elements.len()).ok().map(Range::exact)
                    }
                    Rvalue::Uint8Array(bytes) => i64::try_from(bytes.len()).ok().map(Range::exact),
                    Rvalue::Use(
                        Operand::Copy(Place::Local(source)) | Operand::Move(Place::Local(source)),
                    ) => self.lengths.get(source).copied(),
                    _ => None,
                };
                self.forget(*local);
                if body.locals[local.0].is_captured {
                    return;
                }
                if let Some(read) = read
                    && !read.depends_on(*local)
                {
                    if let ReadKey::Local(source) = read {
                        self.aliases.insert(*local, source);
                    } else {
                        self.reads.insert(read, *local);
                    }
                }
                if let Some(range) = integer {
                    self.integers.insert(*local, range);
                }
                if let Some((array, offset)) = length_value
                    && array != *local
                {
                    self.length_values.insert(*local, (array, offset));
                }
                if let Some(condition) = condition
                    && !condition.left.depends_on(*local)
                    && !condition.right.depends_on(*local)
                {
                    self.conditions.insert(*local, condition);
                }
                if let Some(range) = length {
                    self.lengths.insert(*local, range);
                }
            }
            StatementKind::FreshCell(local) => self.forget(*local),
            // Aliasing writes may invalidate both a container and references
            // obtained through it. Do not try to infer disjointness here.
            StatementKind::Assign { .. }
            | StatementKind::VirtualFieldStore { .. }
            | StatementKind::Intrinsic { .. } => *self = Self::default(),
            StatementKind::Drop(_) | StatementKind::Nop => {}
        }
    }

    fn constrain(&mut self, number: Number, range: Range) {
        match number {
            Number::Constant(_) => {}
            Number::Local(local) => {
                if let Some(range) = self.range(number).intersect(range) {
                    self.integers.insert(local, range);
                }
            }
            Number::Length(local, offset) => {
                let bounds = Range {
                    lo: range.lo.saturating_add(offset),
                    hi: range.hi.saturating_add(offset),
                };
                let old = self.lengths.get(&local).copied().unwrap_or(Range::LENGTH);
                if let Some(range) = old.intersect(bounds) {
                    self.lengths.insert(local, range);
                }
            }
        }
    }

    fn branch(&mut self, condition: Condition, taken: bool) {
        let op = if taken {
            condition.op
        } else {
            invert(condition.op)
        };
        let left = self.range(condition.left);
        let right = self.range(condition.right);
        match op {
            BinOp::Eq => {
                self.constrain(condition.left, right);
                self.constrain(condition.right, left);
            }
            BinOp::Ne => {
                for (number, range, excluded) in [
                    (condition.left, left, right),
                    (condition.right, right, left),
                ] {
                    if excluded.lo == excluded.hi {
                        if range.lo == excluded.lo {
                            self.constrain(
                                number,
                                Range {
                                    lo: range.lo + 1,
                                    ..range
                                },
                            );
                        } else if range.hi == excluded.lo {
                            self.constrain(
                                number,
                                Range {
                                    hi: range.hi - 1,
                                    ..range
                                },
                            );
                        }
                    }
                }
            }
            BinOp::Lt | BinOp::Le => {
                let strict = i64::from(op == BinOp::Lt);
                self.constrain(
                    condition.left,
                    Range {
                        lo: MIN,
                        hi: right.hi - strict,
                    },
                );
                self.constrain(
                    condition.right,
                    Range {
                        lo: left.lo + strict,
                        hi: MAX,
                    },
                );
            }
            BinOp::Gt | BinOp::Ge => self.branch(
                Condition {
                    op: if op == BinOp::Gt {
                        BinOp::Lt
                    } else {
                        BinOp::Le
                    },
                    left: condition.right,
                    right: condition.left,
                },
                true,
            ),
            _ => {}
        }
    }

    fn in_bounds(&self, place: &Place, body: &MirFunctionBody<'_>) -> bool {
        let Place::Index {
            base,
            index,
            kind: IndexKind::Array,
        } = place
        else {
            return false;
        };
        // A completed read can be repeated without trapping until a dependency
        // changes, even when we cannot express its successful bounds test.
        if matches!(self.read_key(place, body), Some(ReadKey::Local(_))) {
            return true;
        }
        let Some(ReadKey::Local(array)) = self.read_key(base, body) else {
            return false;
        };
        if body.locals[array.0].is_captured {
            return false;
        }
        let length = self.lengths.get(&array).copied().unwrap_or(Range::LENGTH);
        let Some(index) = self.number(&Operand::copy_local(*index), body) else {
            return false;
        };
        if let Number::Length(root, offset) = index
            && root == array
        {
            return offset > 0 && offset <= length.lo;
        }
        let index = self.range(index);
        // BAML accepts negative array indices relative to the end.
        index.lo >= -length.lo && index.hi < length.lo
    }

    fn can_discard(&self, value: &Rvalue<'_>, body: &MirFunctionBody<'_>) -> bool {
        value.can_discard_with(|place| self.in_bounds(place, body))
            || matches!(
                value,
                Rvalue::BinaryOp { .. }
                    | Rvalue::UnaryOp {
                        op: UnaryOp::Neg,
                        ..
                    }
            ) && self.integer_value(value, body).is_some()
    }

    fn merge(&mut self, other: &Self, widen: bool) -> bool {
        let before = self.clone();
        self.aliases
            .retain(|local, value| other.aliases.get(local) == Some(value));
        self.reads
            .retain(|key, value| other.reads.get(key) == Some(value));
        self.integers.retain(|local, range| {
            if let Some(other) = other.integers.get(local) {
                *range = range.join(*other, widen, Range::INT);
                true
            } else {
                false
            }
        });
        self.lengths.retain(|local, range| {
            if let Some(other) = other.lengths.get(local) {
                *range = range.join(*other, widen, Range::LENGTH);
                true
            } else {
                false
            }
        });
        self.length_values
            .retain(|local, value| other.length_values.get(local) == Some(value));
        self.conditions
            .retain(|local, value| other.conditions.get(local) == Some(value));
        *self != before
    }
}

fn invert(op: BinOp) -> BinOp {
    match op {
        BinOp::Eq => BinOp::Ne,
        BinOp::Ne => BinOp::Eq,
        BinOp::Lt => BinOp::Ge,
        BinOp::Le => BinOp::Gt,
        BinOp::Gt => BinOp::Le,
        BinOp::Ge => BinOp::Lt,
        _ => unreachable!("only comparisons have branch facts"),
    }
}

pub(super) struct Analysis {
    pub discardable: Vec<Vec<bool>>,
}

impl Analysis {
    pub(super) fn new(body: &MirFunctionBody<'_>) -> Self {
        let handlers: HashSet<_> = body
            .catch_regions
            .iter()
            .map(|region| region.handler)
            .collect();
        let mut entries = vec![None; body.blocks.len()];
        let mut work = VecDeque::new();
        for entry in std::iter::once(body.entry).chain(handlers.iter().copied()) {
            entries[entry.0] = Some(Facts::default());
            work.push_back(entry);
        }
        while let Some(id) = work.pop_front() {
            let mut facts = entries[id.0].clone().unwrap();
            let block = &body.blocks[id.0];
            for statement in &block.statements {
                facts.statement(&statement.kind, body);
            }
            let Some(term) = &block.terminator else {
                continue;
            };
            match term {
                Terminator::Call { .. }
                | Terminator::VirtualCall { .. }
                | Terminator::SysOp { .. }
                | Terminator::Await { .. }
                | Terminator::AwaitAny { .. }
                | Terminator::Spawn { .. } => facts = Facts::default(),
                Terminator::ShortCircuit { destination, .. } => {
                    if let Place::Local(local) = destination {
                        facts.forget(*local);
                    } else {
                        facts = Facts::default();
                    }
                }
                Terminator::NarrowBind { destination, .. } => facts.forget(*destination),
                _ => {}
            }
            for target in term.successors() {
                if target == body.entry || handlers.contains(&target) {
                    continue;
                }
                let mut outgoing = facts.clone();
                if let Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } = term
                    && then_block != else_block
                    && let Some(condition) = facts.condition(condition)
                {
                    outgoing.branch(condition, target == *then_block);
                }
                let changed = if let Some(previous) = &mut entries[target.0] {
                    // Every cycle includes a backwards-numbered edge. Widen
                    // growing ranges there, so loop trip counts do not bound
                    // analysis time. Acyclic backward edges only lose precision.
                    previous.merge(&outgoing, target.0 <= id.0)
                } else {
                    entries[target.0] = Some(outgoing);
                    true
                };
                if changed {
                    work.push_back(target);
                }
            }
        }
        let discardable = body
            .blocks
            .iter()
            .map(|block| {
                let mut facts = entries[block.id.0].clone().unwrap_or_default();
                block
                    .statements
                    .iter()
                    .map(|statement| {
                        let safe = match &statement.kind {
                            StatementKind::Assign { value, .. } => facts.can_discard(value, body),
                            StatementKind::Drop(place) => {
                                facts.can_discard(&Rvalue::Use(Operand::Copy(place.clone())), body)
                            }
                            _ => false,
                        };
                        facts.statement(&statement.kind, body);
                        safe
                    })
                    .collect()
            })
            .collect();
        Self { discardable }
    }
}

/// A local read used only to discard its result has no effect itself. Removing
/// these sinks lets DCE reconsider their producers; potentially trapping
/// producers will get an explicit discard again in `eliminate_dead_locals`.
pub(super) fn remove_trivial_drops(body: &mut MirFunctionBody<'_>) {
    let analysis = Analysis::new(body);
    for block in &mut body.blocks {
        let mut index = 0;
        block.statements.retain(|statement| {
            let remove = matches!(statement.kind, StatementKind::Drop(_))
                && analysis.discardable[block.id.0][index];
            index += 1;
            !remove
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_proofs_cover_extremes_and_interior_values() {
        let values = [MIN, MIN + 1, -3, -1, 0, 1, 3, MAX - 1, MAX];
        for &lo in &values {
            for &hi in values.iter().filter(|&&hi| hi >= lo) {
                for &rhs_lo in &values {
                    for &rhs_hi in values.iter().filter(|&&hi| hi >= rhs_lo) {
                        for op in [BinOp::Add, BinOp::Sub, BinOp::Mul, BinOp::Div, BinOp::Mod] {
                            let Some(proof) = (Range { lo, hi }).arithmetic(
                                op,
                                Range {
                                    lo: rhs_lo,
                                    hi: rhs_hi,
                                },
                            ) else {
                                continue;
                            };
                            for &a in values.iter().filter(|&&n| lo <= n && n <= hi) {
                                for &b in values.iter().filter(|&&n| rhs_lo <= n && n <= rhs_hi) {
                                    let (a, b) = (i128::from(a), i128::from(b));
                                    let result = match op {
                                        BinOp::Add => Some(a + b),
                                        BinOp::Sub => Some(a - b),
                                        BinOp::Mul => Some(a * b),
                                        BinOp::Div => a.checked_div(b),
                                        BinOp::Mod => a.checked_rem(b),
                                        _ => unreachable!(),
                                    }
                                    .expect("proven arithmetic must not divide by zero");
                                    assert!(
                                        (i128::from(proof.lo)..=i128::from(proof.hi))
                                            .contains(&result)
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn joins_widen_only_growing_bounds_and_forget_disagreeing_aliases() {
        let local = Local(1);
        let mut facts = Facts::default();
        facts.integers.insert(local, Range::exact(0));
        facts.aliases.insert(Local(2), local);
        let mut incoming = Facts::default();
        incoming.integers.insert(local, Range { lo: 0, hi: 1 });
        incoming.aliases.insert(Local(2), Local(3));
        assert!(facts.merge(&incoming, true));
        assert_eq!(facts.integers[&local], Range { lo: 0, hi: MAX });
        assert!(facts.aliases.is_empty());
        assert!(!facts.merge(&incoming, true));
    }
}
