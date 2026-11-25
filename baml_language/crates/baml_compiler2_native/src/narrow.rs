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

use baml_compiler2_hir_ty::{
    extern_loc::ClassRef,
    layout::{class_generic_params, class_ref_of, enum_ref_of},
};
use baml_compiler2_mir::{
    BinOp, Constant, Local, Operand, Place, Rvalue, ShortCircuitKind, StatementKind, SwitchKey,
    Terminator, TyTemplate, TyTemplateInterface, TypeTest, UnaryOp,
};
use baml_type::typetag;

use crate::{
    flow::terminator_destination,
    loops::operand_local,
    types::{Body, NativeTy, TypeArg, constant_ty, storage_ty, template_ty},
};

/// What a type test asks of a value in a box.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum TestKind<'db> {
    /// A value of exactly this type.
    Exactly(NativeTy<'db>),
    /// Any array, whatever its elements.
    AnyArray,
    /// A value of the type the function was called with for its type
    /// parameter `.0`, which only the type object it is passed says.
    Argument(usize),
    /// A value that passes any of these tests: a union's members.
    AnyOf(Vec<TestKind<'db>>),
    /// A value of any type that implements this interface.
    Implements(TyTemplateInterface),
    /// A panic: an instance of any class of `baml.panics`, which its
    /// header's flag says; see [`crate::layout::HEADER_PANIC`].
    Panic,
    /// This one value of a literal type: a value of its type equal to it.
    Is(Value),
}

/// The value of a literal type a test asks for.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Value {
    String(String),
    Int(i64),
    Bool(bool),
}

impl<'db> TestKind<'db> {
    pub(crate) fn of(
        db: &'db dyn baml_compiler2_mir::Db,
        test: &TypeTest<'db>,
        arguments: &[TypeArg<'db>],
    ) -> Option<Self> {
        match test {
            TypeTest::Class { class, args } => Self::of_class(db, *class, args, arguments),
            TypeTest::Enum(enum_ref) => Some(Self::Exactly(NativeTy::Enum(*enum_ref))),
            TypeTest::Template(template) => Self::of_template(db, template, arguments),
        }
    }

    fn of_class(
        db: &'db dyn baml_compiler2_mir::Db,
        class: ClassRef<'db>,
        args: &[TyTemplate],
        arguments: &[TypeArg<'db>],
    ) -> Option<Self> {
        if args.len() != class_generic_params(db, class).len() {
            return None;
        }
        let arguments: Vec<_> = args
            .iter()
            .map(|argument| template_ty(db, argument, arguments))
            .collect();
        // Nothing an instance holds tells instantiations at different
        // reference types apart.
        if arguments
            .iter()
            .any(|argument| !matches!(argument, Some(argument) if !argument.is_ref()))
        {
            return None;
        }
        Some(Self::Exactly(NativeTy::Class(class, arguments)))
    }

    fn of_template(
        db: &'db dyn baml_compiler2_mir::Db,
        template: &TyTemplate,
        arguments: &[TypeArg<'db>],
    ) -> Option<Self> {
        Some(Self::Exactly(match template {
            TyTemplate::Int => NativeTy::Int,
            TyTemplate::Float => NativeTy::Float,
            TyTemplate::Bool => NativeTy::Bool,
            TyTemplate::String => NativeTy::String,
            TyTemplate::Bigint => NativeTy::Bigint,
            TyTemplate::Uint8Array => NativeTy::Bytes,
            TyTemplate::Null => NativeTy::Null,
            TyTemplate::Literal(baml_type::Literal::String(text), _) => {
                return Some(Self::Is(Value::String(text.clone())));
            }
            TyTemplate::Literal(baml_type::Literal::Int(value), _) => {
                return Some(Self::Is(Value::Int(*value)));
            }
            TyTemplate::Literal(baml_type::Literal::Bool(value), _) => {
                return Some(Self::Is(Value::Bool(*value)));
            }
            TyTemplate::List(_) => return Some(Self::AnyArray),
            // The function's own: only the type object it is passed says
            // what it is.
            TyTemplate::TypeArgRef(slot) => match arguments.get(*slot as usize) {
                None => return Some(Self::Argument(*slot as usize)),
                Some(_) => return None,
            },
            TyTemplate::Enum(head) => NativeTy::Enum(enum_ref_of(db, head)?),
            TyTemplate::Class(head, args) => {
                return Self::of_class(db, class_ref_of(db, head)?, args, arguments);
            }
            TyTemplate::Interface(name, generics, associated_types) => {
                let whole = generics.iter().all(TyTemplate::is_fully_concrete)
                    && associated_types
                        .iter()
                        .all(|(_, binding)| binding.is_fully_concrete());
                return whole.then(|| {
                    Self::Implements(TyTemplateInterface {
                        name: name.clone(),
                        generics: generics.clone(),
                        associated_types: associated_types.clone(),
                    })
                });
            }
            TyTemplate::Union(members) => {
                let mut tests = Vec::new();
                for member in members {
                    match Self::of_template(db, member, arguments)? {
                        Self::AnyOf(inner) => tests.extend(inner),
                        // Decided by a type object passed in: not one of
                        // a fixed set.
                        Self::Argument(_) => return None,
                        test => tests.push(test),
                    }
                }
                return Some(Self::AnyOf(tests));
            }
            _ => return None,
        }))
    }

    /// Whether the types that pass are known from the test alone.
    pub(crate) fn is_static(&self) -> bool {
        match self {
            Self::Exactly(_) | Self::AnyArray => true,
            Self::Argument(_) | Self::Implements(_) | Self::Panic | Self::Is(_) => false,
            Self::AnyOf(tests) => tests.iter().all(Self::is_static),
        }
    }

    /// What a coarse tag test asks.
    pub(crate) fn of_tag(tag: i64) -> Option<Self> {
        Some(match tag {
            typetag::INT => Self::Exactly(NativeTy::Int),
            typetag::FLOAT => Self::Exactly(NativeTy::Float),
            typetag::BOOL => Self::Exactly(NativeTy::Bool),
            typetag::STRING => Self::Exactly(NativeTy::String),
            typetag::BIGINT => Self::Exactly(NativeTy::Bigint),
            typetag::UINT8ARRAY => Self::Exactly(NativeTy::Bytes),
            typetag::NULL => Self::Exactly(NativeTy::Null),
            typetag::LIST => Self::AnyArray,
            typetag::PANIC => Self::Panic,
            _ => return None,
        })
    }

    pub(crate) fn matches(&self, member: &NativeTy<'db>) -> bool {
        match self {
            Self::Exactly(ty) => ty == member,
            Self::AnyArray => matches!(member, NativeTy::Array(_)),
            // Nothing is known of it where the code is compiled.
            Self::Argument(_) | Self::Implements(_) | Self::Panic | Self::Is(_) => false,
            Self::AnyOf(tests) => tests.iter().any(|test| test.matches(member)),
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
    /// Locals holding the type tag just read of another local.
    tags: HashMap<usize, usize>,
    /// Boxed locals holding the value another boxed local holds: assigned
    /// from it, with neither assigned since. A `match` that binds tests the
    /// copy and reads the original in the arm the test leaves.
    copies: HashMap<usize, usize>,
}

struct Analysis<'a, 'b, 'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    body: &'a Body<'b, 'db>,
    /// The type of each local kept in a box, by local.
    boxed: Vec<Option<NativeTy<'db>>>,
}

impl<'db> Analysis<'_, '_, 'db> {
    /// What a test asks, when the answer says which member a value is.
    fn test(&self, test: &TypeTest<'db>) -> Option<TestKind<'db>> {
        TestKind::of(self.db, test, self.body.arguments).filter(TestKind::is_static)
    }

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
        // What is learnt of a copy is learnt of what it copies.
        if let Some(original) = state.copies.get(&local).copied()
            && original != local
        {
            self.refine_one(state, original, test, passes);
        }
        self.refine_one(state, local, test, passes);
    }

    fn refine_one(&self, state: &mut State<'db>, local: usize, test: &TestKind<'db>, passes: bool) {
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
        state.tags.remove(&local);
        state.tags.retain(|_, read| *read != local);
        state.copies.remove(&local);
        state.copies.retain(|_, original| *original != local);
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
                test: self.test(test)?,
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
            Rvalue::IsType { operand, test } => decided(operand, self.test(test)),
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
        if let Rvalue::TypeTag(Place::Local(read)) = value {
            state.tags.insert(destination.0, read.0);
        }
        if let Rvalue::Use(Operand::Copy(Place::Local(original))) = value
            && original != destination
            && self.is_boxed(destination.0)
            && self.is_boxed(original.0)
        {
            // A copy of a copy is a copy of the first.
            let original = state.copies.get(&original.0).copied().unwrap_or(original.0);
            state.copies.insert(destination.0, original);
        }
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
                let test = self.test(test);
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
            // A switch on the type tag of a local: an arm that one class
            // alone leads to has an instance of it.
            Terminator::Switch {
                discriminant,
                arms,
                otherwise,
                ..
            } if operand_local(discriminant).is_some_and(|tag| state.tags.contains_key(&tag.0)) => {
                let read =
                    operand_local(discriminant).and_then(|tag| state.tags.get(&tag.0).copied());
                let mut edges = vec![(otherwise.0, state.clone())];
                for (key, target) in arms {
                    if edges.iter().any(|(seen, _)| *seen == target.0) {
                        continue;
                    }
                    let mut arriving = state.clone();
                    let alone = *target != *otherwise
                        && arms.iter().filter(|(_, other)| other == target).count() == 1;
                    if let (true, Some(read), SwitchKey::Class(class)) = (alone, read, key)
                        && class_generic_params(self.db, *class).is_empty()
                    {
                        let test = TestKind::Exactly(NativeTy::Class(*class, Vec::new()));
                        self.refine(&mut arriving, read, &test, true);
                    }
                    edges.push((target.0, arriving));
                }
                edges
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
                .filter(NativeTy::holds_any)
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
        let mut edges = analysis.edges(state, terminator);
        // A throw anywhere in the block reaches its handler, with whatever
        // held at that point: nothing is known there.
        if let Some(handler) = contents.unwind {
            edges.push((handler.0, State::default()));
        }
        for (successor, arriving) in edges {
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
