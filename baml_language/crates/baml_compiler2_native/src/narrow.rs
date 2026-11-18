//! Which member of a union a local holds, block by block.
//!
//! MIR reads a local of a union type as the member the type checker proved
//! it holds: after `if (v is int)`, `v + 1` adds the local itself. Values
//! have no tags here, so code generation must know that member to take the
//! value out of its box. This module finds it again from the tests MIR
//! kept: a forward data flow over the type tests, null tests and narrowing
//! binds that reach each block, their negations and their short-circuit
//! combinations.
//!
//! The same walk follows bools whose value no run can change (a test of a
//! constant, which inlining leaves behind) and so finds the blocks no run
//! reaches. Those are not compiled: the dead arm of such a test uses its
//! value as a type it is not.
//!
//! The answer is deliberately coarse: one member (or one member and `null`)
//! per local for a whole block, and none for a local the block assigns.
//! Where it has no answer the local keeps its union type, and a use that
//! needed more is reported as unsupported rather than guessed at.

use std::collections::HashMap;

use baml_compiler2_hir_ty::layout::class_generic_params;
use baml_compiler2_mir::{
    BinOp, Constant, Local, Operand, Place, Rvalue, ShortCircuitKind, StatementKind, Terminator,
    TyTemplate, TypeTest, UnaryOp,
};
use baml_type::typetag;

use crate::{
    flow::terminator_destination,
    loops::operand_local,
    types::{Body, NativeTy, constant_ty, storage_ty},
};

/// What a type test asks of a value in a box.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum TestKind<'db> {
    /// A value of exactly this type.
    Exactly(NativeTy<'db>),
    /// Any array, whatever its elements.
    AnyArray,
}

impl<'db> TestKind<'db> {
    pub(crate) fn of(db: &'db dyn baml_compiler2_mir::Db, test: &TypeTest<'db>) -> Option<Self> {
        Some(Self::Exactly(match test {
            TypeTest::Class { class, args }
                if args.is_empty() && class_generic_params(db, *class).is_empty() =>
            {
                NativeTy::Class(*class)
            }
            TypeTest::Enum(enum_ref) => NativeTy::Enum(*enum_ref),
            TypeTest::Template(TyTemplate::Int) => NativeTy::Int,
            TypeTest::Template(TyTemplate::Float) => NativeTy::Float,
            TypeTest::Template(TyTemplate::Bool) => NativeTy::Bool,
            TypeTest::Template(TyTemplate::String) => NativeTy::String,
            TypeTest::Template(TyTemplate::Null) => NativeTy::Null,
            TypeTest::Template(TyTemplate::List(_)) => return Some(Self::AnyArray),
            _ => return None,
        }))
    }

    /// What a coarse tag test asks.
    pub(crate) fn of_tag(tag: i64) -> Option<Self> {
        Some(match tag {
            typetag::INT => Self::Exactly(NativeTy::Int),
            typetag::FLOAT => Self::Exactly(NativeTy::Float),
            typetag::BOOL => Self::Exactly(NativeTy::Bool),
            typetag::STRING => Self::Exactly(NativeTy::String),
            typetag::NULL => Self::Exactly(NativeTy::Null),
            typetag::LIST => Self::AnyArray,
            _ => return None,
        })
    }

    pub(crate) fn matches(&self, member: &NativeTy<'db>) -> bool {
        match self {
            Self::Exactly(ty) => ty == member,
            Self::AnyArray => matches!(member, NativeTy::Array(_)),
        }
    }
}

/// "This bool is true exactly when `local` passes the test" (or fails it,
/// when negated).
#[derive(Clone, PartialEq, Eq)]
struct Condition<'db> {
    local: usize,
    test: TestKind<'db>,
    negated: bool,
}

/// What is known where a block starts. A local without a fact can hold any
/// member of its type.
#[derive(Clone, PartialEq, Default)]
struct State<'db> {
    facts: HashMap<usize, Vec<NativeTy<'db>>>,
    conditions: HashMap<usize, Condition<'db>>,
    /// Bools with the one value they can have.
    constants: HashMap<usize, bool>,
}

struct Analysis<'a, 'b, 'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    body: &'a Body<'b, 'db>,
    /// The type of each local kept in a box, by local.
    boxed: Vec<Option<NativeTy<'db>>>,
}

impl<'db> Analysis<'_, '_, 'db> {
    fn is_boxed(&self, local: usize) -> bool {
        self.boxed[local].is_some()
    }

    /// Every member `local` can hold, when its type lists them.
    fn members(&self, local: usize) -> Option<Vec<NativeTy<'db>>> {
        match &self.boxed[local] {
            Some(NativeTy::Union(members)) => Some(members.clone()),
            _ => None,
        }
    }

    fn fact(&self, state: &State<'db>, local: usize) -> Option<Vec<NativeTy<'db>>> {
        state
            .facts
            .get(&local)
            .cloned()
            .or_else(|| self.members(local))
    }

    /// Whether `condition` being true tells nothing new in `state`.
    fn holds(&self, state: &State<'db>, bool_local: usize, condition: &Condition<'db>) -> bool {
        state.conditions.get(&bool_local) == Some(condition)
            || self.fact(state, condition.local).is_some_and(|members| {
                members
                    .iter()
                    .all(|member| condition.test.matches(member) != condition.negated)
            })
    }

    fn refine(&self, state: &mut State<'db>, local: usize, test: &TestKind<'db>, passes: bool) {
        if !self.is_boxed(local) {
            return;
        }
        match (self.fact(state, local), test) {
            (Some(mut members), _) => {
                members.retain(|member| test.matches(member) == passes);
                state.facts.insert(local, members);
            }
            (None, TestKind::Exactly(ty)) if passes => {
                state.facts.insert(local, vec![ty.clone()]);
            }
            (None, _) => {}
        }
    }

    fn apply(&self, state: &mut State<'db>, condition: Option<&Condition<'db>>, truth: bool) {
        if let Some(condition) = condition {
            self.refine(
                state,
                condition.local,
                &condition.test,
                truth != condition.negated,
            );
        }
    }

    /// Forget what was known of a local that is about to be assigned.
    fn kill(state: &mut State<'db>, local: usize) {
        state.facts.remove(&local);
        state.conditions.remove(&local);
        state.constants.remove(&local);
        state
            .conditions
            .retain(|_, condition| condition.local != local);
    }

    /// The members an operand's value can be.
    fn operand_fact(
        &self,
        state: &State<'db>,
        operand: &Operand<'db>,
    ) -> Option<Vec<NativeTy<'db>>> {
        let ty = match operand {
            Operand::Constant(constant) => constant_ty(self.db, constant).ok()?,
            Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
                if self.is_boxed(local.0) {
                    return self.fact(state, local.0);
                }
                storage_ty(self.db, self.body, *local).ok()?
            }
            _ => return None,
        };
        Some(match ty {
            NativeTy::Optional(inner) => vec![*inner, NativeTy::Null],
            NativeTy::Union(_) | NativeTy::Unknown | NativeTy::Cell(_) => return None,
            other => vec![other],
        })
    }

    fn condition_of(&self, state: &State<'db>, value: &Rvalue<'db>) -> Option<Condition<'db>> {
        let tested = |operand: &Operand<'db>| {
            operand_local(operand)
                .map(|local| local.0)
                .filter(|local| self.is_boxed(*local))
        };
        match value {
            Rvalue::IsType { operand, test } => Some(Condition {
                local: tested(operand)?,
                test: TestKind::of(self.db, test)?,
                negated: false,
            }),
            Rvalue::IsTypeTag { operand, tag } => Some(Condition {
                local: tested(operand)?,
                test: TestKind::of_tag(*tag)?,
                negated: false,
            }),
            Rvalue::BinaryOp {
                op: op @ (BinOp::Eq | BinOp::Ne),
                left,
                right,
            } => {
                let ((operand, Operand::Constant(Constant::Null))
                | (Operand::Constant(Constant::Null), operand)) = (left, right)
                else {
                    return None;
                };
                Some(Condition {
                    local: tested(operand)?,
                    test: TestKind::Exactly(NativeTy::Null),
                    negated: *op == BinOp::Ne,
                })
            }
            Rvalue::Use(operand) => state.conditions.get(&operand_local(operand)?.0).cloned(),
            Rvalue::UnaryOp {
                op: UnaryOp::Not,
                operand,
            } => {
                let mut condition = state.conditions.get(&operand_local(operand)?.0).cloned()?;
                condition.negated = !condition.negated;
                Some(condition)
            }
            _ => None,
        }
    }

    /// The value of a bool that is the same on every run.
    fn constant_of(&self, state: &State<'db>, value: &Rvalue<'db>) -> Option<bool> {
        let decided = |operand: &Operand<'db>, test: Option<TestKind<'db>>| {
            let (members, test) = (self.operand_fact(state, operand)?, test?);
            let passing = members.iter().filter(|member| test.matches(member)).count();
            if passing == members.len() {
                Some(true)
            } else if passing == 0 {
                Some(false)
            } else {
                None
            }
        };
        match value {
            Rvalue::IsType { operand, test } => decided(operand, TestKind::of(self.db, test)),
            Rvalue::IsTypeTag { operand, tag } => decided(operand, TestKind::of_tag(*tag)),
            Rvalue::Use(Operand::Constant(Constant::Bool(value))) => Some(*value),
            Rvalue::Use(operand) => state.constants.get(&operand_local(operand)?.0).copied(),
            Rvalue::UnaryOp {
                op: UnaryOp::Not,
                operand,
            } => Some(!*state.constants.get(&operand_local(operand)?.0)?),
            _ => None,
        }
    }

    fn statement(&self, state: &mut State<'db>, statement: &StatementKind<'db>) {
        let StatementKind::Assign {
            destination: Place::Local(destination),
            value,
        } = statement
        else {
            return;
        };
        let fact = match value {
            Rvalue::Use(operand) if self.is_boxed(destination.0) => {
                self.operand_fact(state, operand)
            }
            _ => None,
        };
        let condition = self.condition_of(state, value);
        let constant = self.constant_of(state, value);
        Self::kill(state, destination.0);
        if let Some(constant) = constant {
            state.constants.insert(destination.0, constant);
        }
        if let Some(fact) = fact {
            state.facts.insert(destination.0, fact);
        }
        if let Some(condition) = condition
            && condition.local != destination.0
        {
            state.conditions.insert(destination.0, condition);
        }
    }

    /// The state on each edge out of a block that ends in `terminator`.
    fn edges(
        &self,
        mut state: State<'db>,
        terminator: &Terminator<'db>,
    ) -> Vec<(usize, State<'db>)> {
        match terminator {
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                let local = operand_local(condition).map(|local| local.0);
                let constant = match condition {
                    Operand::Constant(Constant::Bool(value)) => Some(*value),
                    _ => local.and_then(|local| state.constants.get(&local).copied()),
                };
                let condition = local.and_then(|local| state.conditions.get(&local).cloned());
                let mut otherwise = state.clone();
                self.apply(&mut state, condition.as_ref(), true);
                self.apply(&mut otherwise, condition.as_ref(), false);
                match constant {
                    Some(true) => vec![(then_block.0, state)],
                    Some(false) => vec![(else_block.0, otherwise)],
                    None => vec![(then_block.0, state), (else_block.0, otherwise)],
                }
            }
            Terminator::NarrowBind {
                source,
                test,
                destination,
                then_block,
                else_block,
            } => {
                let source = operand_local(source).map(|local| local.0);
                let test = TestKind::of(self.db, test);
                let mut otherwise = state.clone();
                for (state, passes) in [(&mut state, true), (&mut otherwise, false)] {
                    if let (Some(source), Some(test)) = (source, &test) {
                        self.refine(state, source, test, passes);
                    }
                    // The destination is bound only where the test passes.
                    if source != Some(destination.0) {
                        let fact = source
                            .filter(|source| passes && self.is_boxed(*source))
                            .and_then(|source| self.fact(state, source));
                        Self::kill(state, destination.0);
                        if let Some(fact) = fact
                            && self.is_boxed(destination.0)
                        {
                            state.facts.insert(destination.0, fact);
                        }
                    }
                }
                vec![(then_block.0, state), (else_block.0, otherwise)]
            }
            Terminator::ShortCircuit {
                operand,
                kind,
                destination,
                eval_rhs,
                join,
            } => {
                let condition = operand_local(operand)
                    .and_then(|local| state.conditions.get(&local.0).cloned());
                // The left operand is the result so far on both edges.
                if let Place::Local(destination) = destination {
                    Self::kill(&mut state, destination.0);
                    if let Some(condition) = &condition
                        && condition.local != destination.0
                    {
                        state.conditions.insert(destination.0, condition.clone());
                    }
                }
                let mut skipped = state.clone();
                match kind {
                    ShortCircuitKind::And => {
                        self.apply(&mut state, condition.as_ref(), true);
                        self.apply(&mut skipped, condition.as_ref(), false);
                    }
                    ShortCircuitKind::Or => {
                        self.apply(&mut state, condition.as_ref(), false);
                        self.apply(&mut skipped, condition.as_ref(), true);
                    }
                    ShortCircuitKind::Coalesce => {}
                }
                vec![(eval_rhs.0, state), (join.0, skipped)]
            }
            other => {
                if let Some(destination) = terminator_destination(other) {
                    Self::kill(&mut state, destination.0);
                }
                other
                    .successors()
                    .into_iter()
                    .map(|successor| (successor.0, state.clone()))
                    .collect()
            }
        }
    }

    /// What holds where two edges meet.
    fn join(&self, known: &State<'db>, arriving: &State<'db>) -> State<'db> {
        let mut joined = State::default();
        for (local, fact) in &known.facts {
            let Some(other) = self.fact(arriving, *local) else {
                continue;
            };
            let mut fact = fact.clone();
            for member in other {
                if !fact.contains(&member) {
                    fact.push(member);
                }
            }
            joined.facts.insert(*local, fact);
        }
        for (local, fact) in &arriving.facts {
            if !known.facts.contains_key(local)
                && let Some(mut members) = self.members(*local)
            {
                // Every member on one edge: nothing known, but said so that
                // two joins of the same edges compare equal.
                members.retain(|member| !fact.contains(member));
                let mut fact = fact.clone();
                fact.extend(members);
                joined.facts.insert(*local, fact);
            }
        }
        for (local, condition) in known.conditions.iter().chain(&arriving.conditions) {
            if self.holds(known, *local, condition) && self.holds(arriving, *local, condition) {
                joined.conditions.insert(*local, condition.clone());
            }
        }
        for (local, constant) in &known.constants {
            if arriving.constants.get(local) == Some(constant) {
                joined.constants.insert(*local, *constant);
            }
        }
        joined
    }
}

/// What the walk found.
pub(crate) struct BlockTypes<'db> {
    /// For each block and local: the type the local is known to hold
    /// throughout the block, when that is narrower than the union it is
    /// declared as. Empty when no local is narrowed anywhere.
    pub(crate) narrowed: Vec<Vec<Option<NativeTy<'db>>>>,
    /// Whether any run reaches each block.
    pub(crate) reachable: Vec<bool>,
}

pub(crate) fn block_types<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &Body<'_, 'db>,
) -> BlockTypes<'db> {
    let nothing = || BlockTypes {
        narrowed: Vec::new(),
        reachable: vec![true; body.blocks.len()],
    };
    let boxed: Vec<Option<NativeTy<'db>>> = (0..body.locals.len())
        .map(|local| {
            storage_ty(db, body, Local(local))
                .ok()
                .filter(NativeTy::is_boxed)
        })
        .collect();
    let analysis = Analysis { db, body, boxed };

    let mut entry_states: Vec<Option<State<'db>>> = vec![None; body.blocks.len()];
    entry_states[body.entry.0] = Some(State::default());
    let mut pending = vec![body.entry.0];
    // Facts only grow and conditions only shrink once facts settle, so this
    // ends; the bound is for a mistake in that argument, and costs only
    // precision.
    let mut budget = body.blocks.len() * 64;
    while let Some(block) = pending.pop() {
        if budget == 0 {
            return nothing();
        }
        budget -= 1;
        let contents = &body.blocks[block];
        let mut state = entry_states[block].clone().unwrap_or_default();
        for statement in &contents.statements {
            analysis.statement(&mut state, &statement.kind);
        }
        let Some(terminator) = &contents.terminator else {
            continue;
        };
        for (successor, arriving) in analysis.edges(state, terminator) {
            let joined = match &entry_states[successor] {
                Some(known) => analysis.join(known, &arriving),
                None => arriving,
            };
            if entry_states[successor].as_ref() != Some(&joined) {
                entry_states[successor] = Some(joined);
                if !pending.contains(&successor) {
                    pending.push(successor);
                }
            }
        }
    }

    let reachable = entry_states.iter().map(Option::is_some).collect();
    let narrowed = body
        .blocks
        .iter()
        .zip(&entry_states)
        .map(|(block, state)| {
            let mut narrowed = vec![None; body.locals.len()];
            let Some(state) = state else {
                return narrowed;
            };
            for (local, fact) in &state.facts {
                narrowed[*local] = match &fact[..] {
                    [only] => Some(only.clone()),
                    [NativeTy::Null, inner] | [inner, NativeTy::Null] => {
                        Some(NativeTy::Optional(Box::new(inner.clone())))
                    }
                    _ => None,
                };
            }
            // A fact about a local is about its value on entry.
            for statement in &block.statements {
                if let StatementKind::Assign {
                    destination: Place::Local(local),
                    ..
                } = &statement.kind
                {
                    narrowed[local.0] = None;
                }
            }
            if let Some(local) = block.terminator.as_ref().and_then(terminator_destination) {
                narrowed[local.0] = None;
            }
            narrowed
        })
        .collect();
    BlockTypes {
        narrowed,
        reachable,
    }
}
