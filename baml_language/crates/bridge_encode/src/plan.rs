//! Deciding what a value becomes on the wire, without doing anything.
//!
//! The planner is a pure function of a read snapshot and a slot: it has no
//! access to the host, so it cannot run host code, register anything or copy
//! a payload. Its product is a [`Node`] tree with a type on every node that
//! states one, and nothing else. A union is decided by trying its members and
//! ranking the fits in one place, [`Planner::tiebreak`]; a generic class or
//! container slot the call left open is solved from the value and then
//! restated with the solution. Because planning has no effects, a plan for
//! the same node under the same slot is computed once and shared.

use std::{collections::HashMap, rc::Rc};

use baml_type::{Literal, MediaKind, Name, ParamTy, WireHead};

use crate::{
    error::{EncodeError, EncodeErrorKind, FaultKind, PathSegment, UnfitKind, bounded},
    facts::{Facts, first_type_var, mentions_type_var, substitute},
    host::{Head, HostValue, LeafKind, Scalar, Shape, SlotTy, StatedTy},
    read::Tree,
    rules::{Answer, ScalarSlot},
};

/// How many planning steps one request may take: a backstop for a value and
/// slot whose trials would otherwise multiply.
pub(crate) const FUEL: usize = 1 << 24;

/// The wire shape of one value, decided. Only the four variants the wire
/// requires a statement on carry a type `T`: a list, map, class or callable
/// node states its type, every other node implies it. While a value is
/// planned a statement is a [`SlotTy`], which may still be open; the plan the
/// root returns states [`StatedTy`]s only. Children are shared: a node
/// planned once under a slot is the same node wherever that plan is reused.
pub(crate) enum Node<'t, V, T = SlotTy> {
    Null,
    /// A scalar; the payload of a string or bytes is read from `value` when
    /// the wire is written.
    Scalar {
        value: &'t Tree<V>,
        scalar: Scalar,
    },
    EnumVariant {
        enumeration: Head,
        variant: String,
    },
    /// A leaf whose payload the host produces at commit.
    Leaf {
        value: &'t Tree<V>,
        kind: LeafKind,
    },
    /// A host function, registered at commit under `signature`.
    Callable {
        value: &'t Tree<V>,
        signature: T,
    },
    List {
        ty: T,
        items: Vec<Child<'t, V, T>>,
    },
    Map {
        ty: T,
        entries: Vec<(KeyNode<'t, V>, Child<'t, V, T>)>,
    },
    Class {
        ty: T,
        fields: Vec<(String, Child<'t, V, T>)>,
    },
}

/// A node under a container or class, shared with every plan that reuses it.
pub(crate) type Child<'t, V, T = SlotTy> = Rc<Node<'t, V, T>>;

/// A map key, decided. The boundary carries string keys only for now (the
/// decoder refuses every other key kind), so this has one variant; a typed
/// key (`map<Color, V>`, `map<int, V>`) is a new variant here and a new rule
/// in [`Planner::key`], nowhere else.
pub(crate) enum KeyNode<'t, V> {
    /// The text of a string key, read from `value` when the wire is written.
    String(&'t Tree<V>),
}

impl<V> Clone for KeyNode<'_, V> {
    fn clone(&self) -> Self {
        match self {
            KeyNode::String(tree) => KeyNode::String(tree),
        }
    }
}

impl<V> Node<'_, V> {
    /// The concrete type this node has, with a [`hole`] where an empty
    /// container gives no evidence of one and [`opaque`] where a handle has
    /// a type only the engine can see.
    pub(crate) fn ty(&self) -> SlotTy {
        match self {
            Node::Null => SlotTy::Null,
            Node::Scalar { scalar, .. } => scalar_ty(scalar),
            Node::EnumVariant { enumeration, .. } => {
                SlotTy::Enum(WireHead::Static(enumeration.clone()))
            }
            Node::Leaf { kind, .. } => match kind {
                LeafKind::Handle => opaque(),
                LeafKind::Media(kind) => SlotTy::Media(*kind),
                LeafKind::Prompt => SlotTy::PromptAst,
                LeafKind::TypeValue => SlotTy::Type,
            },
            Node::Callable { signature, .. } => signature.clone(),
            Node::List { ty, .. } | Node::Map { ty, .. } | Node::Class { ty, .. } => ty.clone(),
        }
    }
}

/// One step from the node a failure is about back up to the root; pushed as
/// the failure unwinds, so a success allocates nothing.
pub(crate) enum Step<'t, V> {
    Field(String),
    Index(usize),
    /// A map key itself.
    Key(&'t Tree<V>),
    /// A map entry's value, under its key.
    Entry(&'t Tree<V>),
}

impl<V> Clone for Step<'_, V> {
    fn clone(&self) -> Self {
        match self {
            Step::Field(name) => Step::Field(name.clone()),
            Step::Index(index) => Step::Index(*index),
            Step::Key(key) => Step::Key(key),
            Step::Entry(key) => Step::Entry(key),
        }
    }
}

/// Why a walk did not produce a node.
pub(crate) struct Failure<'t, V: HostValue> {
    /// Innermost first.
    pub steps: Vec<Step<'t, V>>,
    pub kind: FailKind<'t, V>,
}

pub(crate) enum FailKind<'t, V: HostValue> {
    /// Not a member of the slot's type: a union tries its other members.
    Unfit(UnfitKind<&'t Tree<V>>),
    /// A fault in the facts or the value: final.
    Fault(FaultKind),
    /// The host raised while answering (reading a string to compare).
    Host(V::Error),
}

impl<'t, V: HostValue> Failure<'t, V> {
    fn unfit(kind: UnfitKind<&'t Tree<V>>) -> Box<Self> {
        Box::new(Failure {
            steps: Vec::new(),
            kind: FailKind::Unfit(kind),
        })
    }

    fn fault(kind: FaultKind) -> Box<Self> {
        Box::new(Failure {
            steps: Vec::new(),
            kind: FailKind::Fault(kind),
        })
    }

    fn host(error: V::Error) -> Box<Self> {
        Box::new(Failure {
            steps: Vec::new(),
            kind: FailKind::Host(error),
        })
    }

    fn at(mut self: Box<Self>, step: Step<'t, V>) -> Box<Self> {
        self.steps.push(step);
        self
    }

    fn is_unfit(&self) -> bool {
        matches!(self.kind, FailKind::Unfit(_))
    }

    /// Render for the caller, under `root`: every step's key is described
    /// here, once, and never for a failure a union discarded. Every piece of
    /// host text in the result is bounded.
    pub(crate) fn into_error(self, root: PathSegment) -> crate::error::Error<V::Error> {
        let mut path = vec![root];
        for step in self.steps.into_iter().rev() {
            path.push(match step {
                Step::Field(name) => PathSegment::Field(bounded(name)),
                Step::Index(index) => PathSegment::Index(index),
                Step::Key(key) => PathSegment::Key(bounded(key.value.describe())),
                Step::Entry(key) => PathSegment::Field(bounded(
                    key.value
                        .text()
                        .map_or_else(|_| key.value.describe(), std::borrow::Cow::into_owned),
                )),
            });
        }
        match self.kind {
            FailKind::Host(error) => crate::error::Error::Host(error),
            FailKind::Unfit(kind) => crate::error::Error::Value(EncodeError {
                path,
                kind: Box::new(EncodeErrorKind::Unfit(
                    kind.map_found(|tree| bounded(tree.value.describe())),
                )),
            }),
            FailKind::Fault(kind) => crate::error::Error::Value(EncodeError {
                path,
                kind: Box::new(EncodeErrorKind::Fault(kind)),
            }),
        }
    }
}

/// A failure is boxed: the `Ok` arm is the hot one, and a failure carries
/// whole types.
type Planned<'t, V> = Result<Rc<Node<'t, V>>, Box<Failure<'t, V>>>;

/// A whole value's plan: every statement decided and realized.
pub(crate) type Decided<'t, V> = Result<Rc<Node<'t, V, StatedTy>>, Box<Failure<'t, V>>>;

/// How a shape stands to a union member before planning it: the order
/// [`Planner::tiebreak`] prefers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    /// The value knows its type and the member is it.
    Identity,
    /// The member is the structural type of the value's shape.
    Natural,
    /// The walker cannot judge the member (an interface, a type variable,
    /// `unknown`, a handle): the value goes as it is and the engine decides,
    /// which beats changing it. Under `T | float` a Python `1` is an `int`
    /// that binds `T`, as the compiler types the literal.
    Open,
    /// The member takes the value only by the language's conversion.
    Conversion,
}

/// A planning memo key: the node (by address, which the snapshot keeps
/// stable) and the slot.
type MemoKey = (usize, SlotTy);

pub(crate) struct Planner<'f, 't, V: HostValue> {
    facts: &'f Facts,
    fuel: usize,
    /// Every plan so far, shared: a union or a solve that plans a node again
    /// under the same slot gets the same node back.
    memo: HashMap<MemoKey, Memo<'t, V>>,
}

enum Memo<'t, V> {
    Fit(Rc<Node<'t, V>>),
    Unfit(Vec<Step<'t, V>>, UnfitKind<&'t Tree<V>>),
}

impl<'f, 't, V: HostValue> Planner<'f, 't, V> {
    pub(crate) fn new(facts: &'f Facts) -> Self {
        Planner {
            facts,
            fuel: FUEL,
            memo: HashMap::new(),
        }
    }

    /// Plan a whole value against `slot`. A type variable the call left open
    /// is bound by what the value says; a value that says nothing (an empty
    /// container) or whose type only the engine can see (a handle inside a
    /// container) leaves it unbound, which is refused here: the caller names
    /// the type. Every statement of the plan returned is realized.
    pub(crate) fn plan_root(&mut self, tree: &'t Tree<V>, slot: &SlotTy) -> Decided<'t, V> {
        let node = self.plan(tree, slot)?;
        // A bare handle states nothing and is the engine's to judge: under an
        // open slot (`self: Stream<T>`) the engine binds the variable from
        // the live value.
        let engine_binds = matches!(
            *node,
            Node::Leaf {
                kind: LeafKind::Handle,
                ..
            }
        );
        if !engine_binds && has_open(&node.ty()) && mentions_type_var(slot) {
            return Err(Failure::unfit(UnfitKind::UnboundTypeArgs {
                var: first_type_var(slot)
                    .map_or_else(|| "T".to_string(), |param| param.name().to_string()),
            }));
        }
        self.decide(&node).map_err(Failure::fault)
    }

    /// `node` with every statement in stated form and realized. The root
    /// refused every variable the value left open, so a statement that is not
    /// realized here is the walker's own fault.
    fn decide(&self, node: &Node<'t, V>) -> Result<Rc<Node<'t, V, StatedTy>>, FaultKind> {
        let state = |ty: &SlotTy| {
            let ty = self.facts.stated_form(ty.clone());
            StatedTy::try_from(&ty).map_err(|_| FaultKind::Undecided {
                ty: bounded(ty.to_string()),
            })
        };
        Ok(Rc::new(match node {
            Node::Null => Node::Null,
            Node::Scalar { value, scalar } => Node::Scalar {
                value,
                scalar: scalar.clone(),
            },
            Node::EnumVariant {
                enumeration,
                variant,
            } => Node::EnumVariant {
                enumeration: enumeration.clone(),
                variant: variant.clone(),
            },
            Node::Leaf { value, kind } => Node::Leaf { value, kind: *kind },
            Node::Callable { value, signature } => Node::Callable {
                value,
                signature: state(signature)?,
            },
            Node::List { ty, items } => Node::List {
                ty: state(ty)?,
                items: items
                    .iter()
                    .map(|item| self.decide(item))
                    .collect::<Result<_, _>>()?,
            },
            Node::Map { ty, entries } => Node::Map {
                ty: state(ty)?,
                entries: entries
                    .iter()
                    .map(|(key, item)| Ok((key.clone(), self.decide(item)?)))
                    .collect::<Result<_, FaultKind>>()?,
            },
            Node::Class { ty, fields } => Node::Class {
                ty: state(ty)?,
                fields: fields
                    .iter()
                    .map(|(name, item)| Ok((name.clone(), self.decide(item)?)))
                    .collect::<Result<_, FaultKind>>()?,
            },
        }))
    }

    /// Plan `tree` against `slot`. The node's type may carry a [`hole`] or an
    /// [`opaque`] where `slot` mentions a type variable nothing in the value
    /// binds; a consumer solving that variable fills a hole from the value's
    /// siblings, and the root refuses what stays open.
    fn plan(&mut self, tree: &'t Tree<V>, slot: &SlotTy) -> Planned<'t, V> {
        match self.fuel.checked_sub(1) {
            Some(left) => self.fuel = left,
            None => return Err(Failure::fault(FaultKind::TooMuchPlanning { budget: FUEL })),
        }
        let key = (std::ptr::from_ref(tree) as usize, slot.clone());
        match self.memo.get(&key) {
            Some(Memo::Fit(node)) => return Ok(Rc::clone(node)),
            Some(Memo::Unfit(steps, kind)) => {
                return Err(Box::new(Failure {
                    steps: steps.clone(),
                    kind: FailKind::Unfit(kind.clone()),
                }));
            }
            None => {}
        }
        let planned = self.plan_uncached(tree, slot);
        match &planned {
            Ok(node) => {
                self.memo.insert(key, Memo::Fit(Rc::clone(node)));
            }
            Err(failure) => {
                if let FailKind::Unfit(kind) = &failure.kind {
                    self.memo
                        .insert(key, Memo::Unfit(failure.steps.clone(), kind.clone()));
                }
            }
        }
        planned
    }

    fn plan_uncached(&mut self, tree: &'t Tree<V>, slot: &SlotTy) -> Planned<'t, V> {
        let slot = self.facts.unfold(slot);
        if let Shape::Leaf(kind) = tree.shape {
            return self.leaf_in(tree, kind, slot);
        }
        match slot {
            // The value as the language says it is. A hole or an opaque type
            // is `unknown`, as the compiler types `[]` with no annotation.
            SlotTy::Unknown | SlotTy::Interface(..) => {
                let node = self.natural(tree)?;
                let ty = fill(&node.ty(), &SlotTy::Unknown);
                Ok(restate(node, ty))
            }
            // A type variable the call left open: the value's own type is the
            // evidence that binds it, holes included.
            SlotTy::TypeVar(_) => self.natural(tree),
            SlotTy::Union(members) => {
                let members = self.facts.flatten(members);
                self.union(tree, &members)
            }
            SlotTy::Null => match tree.shape {
                Shape::Null => Ok(Rc::new(Node::Null)),
                _ => Err(mismatch(tree, slot)),
            },
            SlotTy::Int => Self::scalar(tree, slot, ScalarSlot::Int),
            SlotTy::Bigint => Self::scalar(tree, slot, ScalarSlot::Bigint),
            SlotTy::Float => Self::scalar(tree, slot, ScalarSlot::Float),
            SlotTy::Bool => Self::scalar(tree, slot, ScalarSlot::Bool),
            SlotTy::String => Self::scalar(tree, slot, ScalarSlot::String),
            SlotTy::Uint8Array => Self::scalar(tree, slot, ScalarSlot::Bytes),
            SlotTy::Literal(literal, _) => Self::literal(tree, slot, literal),
            SlotTy::Enum(WireHead::Static(name)) => self.enumeration(tree, slot, name, None),
            SlotTy::EnumVariant(WireHead::Static(name), variant) => {
                self.enumeration(tree, slot, name, Some(variant.as_str()))
            }
            SlotTy::List(inner) => self.list(tree, slot, inner),
            SlotTy::Map { key, value } => self.map(tree, slot, key, value),
            SlotTy::Class(WireHead::Static(name), args) => self.class(tree, slot, name, args),
            // A declaration the engine created at run time: the facts know
            // nothing of it, and only a value the engine handed out, a
            // handle, inhabits it. Handles were taken above.
            SlotTy::Enum(WireHead::Live(_) | WireHead::Anonymous(_))
            | SlotTy::EnumVariant(WireHead::Live(_) | WireHead::Anonymous(_), _)
            | SlotTy::Class(WireHead::Live(_) | WireHead::Anonymous(_), _) => {
                Err(Failure::unfit(UnfitKind::RuntimeDeclaration { found: tree }))
            }
            SlotTy::Function { .. } => Self::callable(tree, slot),
            // Only a leaf inhabits these, and leaves were handled above.
            SlotTy::Media(_)
            | SlotTy::PromptAst
            | SlotTy::Type
            | SlotTy::Resource
            | SlotTy::RustType
            | SlotTy::Future(..)
            | SlotTy::Never
            | SlotTy::AssociatedTypeProjection { .. }
            // A recursive alias the facts do not know: nothing to judge against.
            | SlotTy::TypeAlias(_) => Err(mismatch(tree, slot)),
        }
    }

    // ── Slots that fix nothing ──────────────────────────────────────────────

    /// The value's natural node: its own type, with a [`hole`] where an empty
    /// container gives no evidence of one and an [`opaque`] where a handle's
    /// type is the engine's. A container states the one type its elements
    /// share, their union otherwise; an empty element takes the type its
    /// siblings agree on (`[[], [1]]` is `int[][]`); a handle never does.
    fn natural(&mut self, tree: &'t Tree<V>) -> Planned<'t, V> {
        let node = match &tree.shape {
            Shape::Null => Node::Null,
            Shape::Scalar(scalar) => match V::RULES.fit(scalar, ScalarSlot::Open) {
                Answer::AsIs(scalar) | Answer::Converted(scalar) => Node::Scalar {
                    value: tree,
                    scalar,
                },
                Answer::No => return Err(Failure::unfit(UnfitKind::NoBamlType { found: tree })),
            },
            Shape::EnumMember {
                enumeration,
                variant,
            } => {
                // The facts may not reach this enum (a declaration the
                // program made at run time): the host names what it has.
                if let Some(facts) = self.facts.enumeration(enumeration)
                    && !facts.variants.contains(variant)
                {
                    return Err(Failure::unfit(UnfitKind::UnknownVariant {
                        enumeration: enumeration.clone(),
                        variant: bounded(variant.clone()),
                    }));
                }
                Node::EnumVariant {
                    enumeration: enumeration.clone(),
                    variant: variant.clone(),
                }
            }
            Shape::List(items) => {
                let mut nodes = Vec::with_capacity(items.len());
                for (index, item) in items.iter().enumerate() {
                    nodes.push(
                        self.natural(item)
                            .map_err(|failure| failure.at(Step::Index(index)))?,
                    );
                }
                let element = join(&mut nodes);
                Node::List {
                    ty: SlotTy::List(Box::new(element)),
                    items: nodes,
                }
            }
            Shape::Map(entries) => {
                let mut keys = Vec::with_capacity(entries.len());
                let mut nodes = Vec::with_capacity(entries.len());
                for (key, item) in entries {
                    keys.push(self.key(key, &SlotTy::String)?);
                    nodes.push(
                        self.natural(item)
                            .map_err(|failure| failure.at(Step::Entry(key)))?,
                    );
                }
                let value = join(&mut nodes);
                Node::Map {
                    ty: SlotTy::Map {
                        key: Box::new(SlotTy::String),
                        value: Box::new(value),
                    },
                    entries: keys.into_iter().zip(nodes).collect(),
                }
            }
            Shape::Instance {
                class,
                type_args,
                fields,
            } => match self.facts.class(class) {
                Some(facts) => {
                    let facts = facts.clone();
                    // The arguments the value knows, or none: the fields decide.
                    let fixed: Vec<Option<SlotTy>> = match type_args {
                        Some(args) => self
                            .host_type_args(&facts, args)?
                            .into_iter()
                            .map(Some)
                            .collect(),
                        None => vec![None; facts.generic_params.len()],
                    };
                    let present: Vec<(String, &'t Tree<V>)> = fields
                        .iter()
                        .map(|(name, field)| (name.clone(), field))
                        .collect();
                    return self.fields(&facts, &fixed, &present, FieldSource::Instance);
                }
                // The facts do not reach this class: the host names it and
                // its fields are what they are, nothing here to fill a hole
                // in one; the engine checks them against the class it knows.
                None => {
                    let args: Vec<SlotTy> = match type_args {
                        Some(args) => args.iter().map(|arg| self.host_stated(arg)).collect(),
                        None => Vec::new(),
                    };
                    let mut nodes = Vec::with_capacity(fields.len());
                    for (name, field) in fields {
                        let node = self
                            .natural(field)
                            .map_err(|failure| failure.at(Step::Field(name.clone())))?;
                        let ty = fill(&node.ty(), &SlotTy::Unknown);
                        nodes.push((name.clone(), restate(node, ty)));
                    }
                    Node::Class {
                        ty: SlotTy::Class(WireHead::Static(class.clone()), args.into()),
                        fields: nodes,
                    }
                }
            },
            // A callable has no type of its own: only a function slot gives
            // it one, and an open slot fixes none.
            Shape::Callable => {
                return Err(Failure::unfit(UnfitKind::UntypedCallable { found: tree }));
            }
            // A handle's type is the engine's to know: as evidence it is
            // opaque, which no sibling fills.
            Shape::Leaf(kind) => Node::Leaf {
                value: tree,
                kind: *kind,
            },
            Shape::Other => return Err(Failure::unfit(UnfitKind::NoBamlType { found: tree })),
        };
        Ok(Rc::new(node))
    }

    /// A map key against the key slot. The boundary carries string keys only
    /// for now (the decoder refuses every other kind), so this is the one
    /// place that rule lives: the slot has to be one a string inhabits, and
    /// a key that is not a string is reported as a key.
    fn key(
        &mut self,
        key: &'t Tree<V>,
        slot: &SlotTy,
    ) -> Result<KeyNode<'t, V>, Box<Failure<'t, V>>> {
        let node = self.plan(key, slot).map_err(|failure| {
            let failure = failure.at(Step::Key(key));
            match failure.kind {
                FailKind::Unfit(UnfitKind::Mismatch { expected, found }) => Box::new(Failure {
                    steps: failure.steps,
                    kind: FailKind::Unfit(UnfitKind::MapKey { expected, found }),
                }),
                FailKind::Unfit(_) | FailKind::Fault(_) | FailKind::Host(_) => failure,
            }
        })?;
        match &*node {
            Node::Scalar {
                scalar: Scalar::String,
                ..
            } => Ok(KeyNode::String(key)),
            Node::Null
            | Node::Scalar { .. }
            | Node::EnumVariant { .. }
            | Node::Leaf { .. }
            | Node::Callable { .. }
            | Node::List { .. }
            | Node::Map { .. }
            | Node::Class { .. } => Err(Failure::unfit(UnfitKind::MapKey {
                expected: slot.clone(),
                found: key,
            })
            .at(Step::Key(key))),
        }
    }

    // ── Host-supplied types ─────────────────────────────────────────────────

    /// A type the host spelled (an instance's own type arguments), in stated
    /// form. A host spells realized types only, so no host-chosen variable
    /// ever reaches a solve.
    fn host_stated(&self, ty: &StatedTy) -> SlotTy {
        self.facts.stated_form(SlotTy::from(ty))
    }

    /// An instance's own type arguments for `class`, in stated form and of
    /// the class's arity.
    fn host_type_args(
        &self,
        class: &crate::facts::ClassFacts,
        args: &[StatedTy],
    ) -> Result<Vec<SlotTy>, Box<Failure<'t, V>>> {
        if args.len() != class.generic_params.len() {
            return Err(Failure::fault(FaultKind::ClassArity {
                class: class.name.clone(),
                declared: class.generic_params.len(),
                stated: args.len(),
            }));
        }
        Ok(args.iter().map(|arg| self.host_stated(arg)).collect())
    }

    // ── Unions ──────────────────────────────────────────────────────────────

    fn union<'m>(&mut self, tree: &'t Tree<V>, members: &[&'m SlotTy]) -> Planned<'t, V> {
        let mut ranked: Vec<(Rank, &'m SlotTy)> = members
            .iter()
            .filter_map(|member| self.rank(tree, member).map(|rank| (rank, *member)))
            .collect();
        ranked.sort_by_key(|(rank, _)| *rank);
        // The one member that took the value's shape but refused something
        // inside it: its own failure is the useful one to report.
        let mut inner: Option<Box<Failure<'t, V>>> = None;
        let mut index = 0;
        while index < ranked.len() {
            let rank = ranked[index].0;
            let mut fits = Vec::new();
            while index < ranked.len() && ranked[index].0 == rank {
                let member = ranked[index].1;
                index += 1;
                match self.plan(tree, member) {
                    Ok(node) => fits.push((member, node)),
                    Err(failure) if failure.is_unfit() => {
                        if !failure.steps.is_empty() {
                            inner = match inner {
                                None => Some(failure),
                                Some(_) => Some(Failure::unfit(UnfitKind::NoMember {
                                    members: members.iter().map(|m| (*m).clone()).collect(),
                                    found: tree,
                                })),
                            };
                        }
                    }
                    Err(failure) => return Err(failure),
                }
            }
            if !fits.is_empty() {
                return self.tiebreak(tree, fits);
            }
        }
        match inner {
            Some(failure)
                if !matches!(failure.kind, FailKind::Unfit(UnfitKind::NoMember { .. })) =>
            {
                Err(failure)
            }
            _ => Err(Failure::unfit(UnfitKind::NoMember {
                members: members.iter().map(|m| (*m).clone()).collect(),
                found: tree,
            })),
        }
    }

    /// Choose among the members of one rank that took the value: the one
    /// place a union member is chosen. Candidates whose planned type is the
    /// same type count as one (a scalar under two members, `null` under
    /// `int? | string?`; unions compare as sets). Among distinct types, the
    /// one equal to the value's natural type wins, as the compiler would type
    /// the literal; otherwise the value is `Ambiguous` among exactly those.
    fn tiebreak<'m>(
        &mut self,
        tree: &'t Tree<V>,
        mut fits: Vec<(&'m SlotTy, Rc<Node<'t, V>>)>,
    ) -> Planned<'t, V> {
        let mut distinct: Vec<(SlotTy, usize)> = Vec::new();
        for (index, (_, node)) in fits.iter().enumerate() {
            let ty = node.ty();
            if !distinct.iter().any(|(seen, _)| equivalent(seen, &ty)) {
                distinct.push((ty, index));
            }
        }
        let chosen = match distinct.as_slice() {
            [] => unreachable!("tiebreak is called with at least one fit"),
            [(_, index)] => *index,
            _ => {
                // The value's own type, computed only now that it decides.
                let natural = self.natural(tree).ok().map(|node| node.ty());
                match natural
                    .and_then(|natural| distinct.iter().find(|(ty, _)| equivalent(ty, &natural)))
                {
                    Some((_, index)) => *index,
                    None => {
                        return Err(Failure::unfit(UnfitKind::Ambiguous {
                            members: distinct
                                .iter()
                                .map(|(_, index)| fits[*index].0.clone())
                                .collect(),
                            found: tree,
                        }));
                    }
                }
            }
        };
        Ok(fits.swap_remove(chosen).1)
    }

    /// How `tree`'s shape stands to `member`, or `None` when the shape can
    /// never take it (a list under `int`), which saves planning it.
    fn rank(&self, tree: &Tree<V>, member: &SlotTy) -> Option<Rank> {
        let member = self.facts.unfold(member);
        if matches!(
            member,
            SlotTy::Unknown | SlotTy::Interface(..) | SlotTy::TypeVar(_)
        ) {
            return Some(Rank::Open);
        }
        match (&tree.shape, member) {
            (Shape::Null, SlotTy::Null) => Some(Rank::Identity),
            (Shape::Null, _) => None,
            (Shape::Scalar(scalar), member) => scalar_rank::<V>(scalar, member),
            (Shape::EnumMember { enumeration, .. }, SlotTy::Enum(WireHead::Static(name)))
            | (
                Shape::EnumMember { enumeration, .. },
                SlotTy::EnumVariant(WireHead::Static(name), _),
            ) if enumeration == name => Some(Rank::Identity),
            (Shape::EnumMember { .. }, _) => None,
            (Shape::List(_), SlotTy::List(_)) => Some(Rank::Natural),
            (Shape::List(_), _) => None,
            (Shape::Map(_), SlotTy::Map { .. }) => Some(Rank::Natural),
            (Shape::Map(_), SlotTy::Class(..)) => Some(Rank::Conversion),
            (Shape::Map(_), _) => None,
            (Shape::Instance { class, .. }, SlotTy::Class(WireHead::Static(name), _))
                if class == name =>
            {
                Some(Rank::Identity)
            }
            (Shape::Instance { .. }, _) => None,
            (Shape::Callable, SlotTy::Function { .. }) => Some(Rank::Natural),
            (Shape::Callable, _) => None,
            (Shape::Leaf(LeafKind::Handle), _) => Some(Rank::Open),
            (Shape::Leaf(LeafKind::Media(kind)), SlotTy::Media(declared))
                if *declared == MediaKind::Generic || declared == kind =>
            {
                Some(Rank::Identity)
            }
            (Shape::Leaf(LeafKind::Media(kind)), SlotTy::Class(WireHead::Static(name), args))
                if args.is_empty()
                    && MediaKind::from_wrapper_class_name(&name.render_dotted(false))
                        .is_some_and(|wrapper| {
                            wrapper == MediaKind::Generic || wrapper == *kind
                        }) =>
            {
                Some(Rank::Identity)
            }
            (Shape::Leaf(LeafKind::Prompt), SlotTy::PromptAst)
            | (Shape::Leaf(LeafKind::TypeValue), SlotTy::Type) => Some(Rank::Identity),
            (Shape::Leaf(LeafKind::Prompt), SlotTy::Class(WireHead::Static(name), args))
                if args.is_empty() && is_prompt_class(name) =>
            {
                Some(Rank::Identity)
            }
            (Shape::Leaf(_), _) => None,
            (Shape::Other, _) => None,
        }
    }

    // ── Scalars, literals, enums ────────────────────────────────────────────

    fn scalar(tree: &'t Tree<V>, slot: &SlotTy, kind: ScalarSlot) -> Planned<'t, V> {
        let Shape::Scalar(scalar) = &tree.shape else {
            return Err(mismatch(tree, slot));
        };
        match V::RULES.fit(scalar, kind) {
            Answer::AsIs(scalar) | Answer::Converted(scalar) => Ok(Rc::new(Node::Scalar {
                value: tree,
                scalar,
            })),
            Answer::No => Err(mismatch(tree, slot)),
        }
    }

    fn literal(tree: &'t Tree<V>, slot: &SlotTy, literal: &Literal) -> Planned<'t, V> {
        let Shape::Scalar(scalar) = &tree.shape else {
            return Err(mismatch(tree, slot));
        };
        let kind = match literal {
            Literal::Int(_) => ScalarSlot::Int,
            Literal::Bigint(_) => ScalarSlot::Bigint,
            // No float value inhabits a float literal type: the parser admits
            // none and the engine judges none a member.
            Literal::Float(_) => {
                return Err(Failure::unfit(UnfitKind::LiteralMismatch {
                    expected: slot.clone(),
                    found: tree,
                }));
            }
            Literal::String(_) => ScalarSlot::String,
            Literal::Bool(_) => ScalarSlot::Bool,
        };
        let scalar = match V::RULES.fit(scalar, kind) {
            Answer::AsIs(scalar) | Answer::Converted(scalar) => scalar,
            Answer::No => return Err(mismatch(tree, slot)),
        };
        let equal = match (literal, &scalar) {
            (Literal::Int(expected), Scalar::Int(found)) => *expected == found.get(),
            (Literal::Bigint(expected), Scalar::BigInt(found)) => expected == found,
            (Literal::Bool(expected), Scalar::Bool(found)) => expected == found,
            (Literal::String(expected), Scalar::String) => {
                let text = tree.value.text().map_err(Failure::host)?;
                *expected == *text
            }
            (
                Literal::Int(_)
                | Literal::Bigint(_)
                | Literal::Float(_)
                | Literal::String(_)
                | Literal::Bool(_),
                _,
            ) => {
                debug_assert!(false, "a rule answered a {kind:?} slot with {scalar:?}");
                false
            }
        };
        if !equal {
            return Err(Failure::unfit(UnfitKind::LiteralMismatch {
                expected: slot.clone(),
                found: tree,
            }));
        }
        Ok(Rc::new(Node::Scalar {
            value: tree,
            scalar,
        }))
    }

    fn enumeration(
        &mut self,
        tree: &'t Tree<V>,
        slot: &SlotTy,
        name: &Head,
        only: Option<&str>,
    ) -> Planned<'t, V> {
        let Some(facts) = self.facts.enumeration(name) else {
            return Err(Failure::fault(FaultKind::UnknownEnum(name.clone())));
        };
        let variant: String = match &tree.shape {
            Shape::EnumMember {
                enumeration,
                variant,
            } if enumeration == name => variant.clone(),
            // A string names a variant, by the language's convention for
            // enums it has no value of its own for.
            Shape::Scalar(Scalar::String) => tree.value.text().map_err(Failure::host)?.into_owned(),
            _ => return Err(mismatch(tree, slot)),
        };
        if !facts.variants.contains(&variant) {
            return Err(Failure::unfit(UnfitKind::UnknownVariant {
                enumeration: name.clone(),
                variant: bounded(variant),
            }));
        }
        if only.is_some_and(|only| only != variant) {
            return Err(Failure::unfit(UnfitKind::LiteralMismatch {
                expected: slot.clone(),
                found: tree,
            }));
        }
        Ok(Rc::new(Node::EnumVariant {
            enumeration: name.clone(),
            variant,
        }))
    }

    // ── Containers ──────────────────────────────────────────────────────────

    fn list(&mut self, tree: &'t Tree<V>, slot: &SlotTy, inner: &SlotTy) -> Planned<'t, V> {
        let Shape::List(items) = &tree.shape else {
            return Err(mismatch(tree, slot));
        };
        let inner = if mentions_type_var(inner) {
            // The slot leaves the element type open: solve what the elements
            // say, then plan them against the solution. A variable no element
            // binds is a hole, for the container above to fill.
            let mut found = Vec::with_capacity(items.len());
            for (index, item) in items.iter().enumerate() {
                let ty = self
                    .plan(item, inner)
                    .map_err(|failure| failure.at(Step::Index(index)))?
                    .ty();
                push_distinct(&mut found, ty);
            }
            let mut solved = Solution::for_template(inner);
            solved.learn(inner, &evidence(&found));
            holes_for_vars(&solved.apply(inner))
        } else {
            inner.clone()
        };
        let mut nodes = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            nodes.push(
                self.plan(item, &inner)
                    .map_err(|failure| failure.at(Step::Index(index)))?,
            );
        }
        Ok(Rc::new(Node::List {
            ty: SlotTy::List(Box::new(inner)),
            items: nodes,
        }))
    }

    fn map(
        &mut self,
        tree: &'t Tree<V>,
        slot: &SlotTy,
        key_slot: &SlotTy,
        value_slot: &SlotTy,
    ) -> Planned<'t, V> {
        let Shape::Map(entries) = &tree.shape else {
            return Err(mismatch(tree, slot));
        };
        let value_slot = if mentions_type_var(value_slot) {
            let mut found = Vec::with_capacity(entries.len());
            for (key, item) in entries {
                let ty = self
                    .plan(item, value_slot)
                    .map_err(|failure| failure.at(Step::Entry(key)))?
                    .ty();
                push_distinct(&mut found, ty);
            }
            let mut solved = Solution::for_template(value_slot);
            solved.learn(value_slot, &evidence(&found));
            holes_for_vars(&solved.apply(value_slot))
        } else {
            value_slot.clone()
        };
        // Keys are strings on the wire, so a key variable is a string.
        let key_slot = if mentions_type_var(key_slot) {
            SlotTy::String
        } else {
            key_slot.clone()
        };
        let mut nodes = Vec::with_capacity(entries.len());
        for (key, item) in entries {
            let key_node = self.key(key, &key_slot)?;
            let node = self
                .plan(item, &value_slot)
                .map_err(|failure| failure.at(Step::Entry(key)))?;
            nodes.push((key_node, node));
        }
        Ok(Rc::new(Node::Map {
            ty: SlotTy::Map {
                key: Box::new(key_slot),
                value: Box::new(value_slot),
            },
            entries: nodes,
        }))
    }

    fn class(
        &mut self,
        tree: &'t Tree<V>,
        slot: &SlotTy,
        name: &Head,
        slot_args: &[SlotTy],
    ) -> Planned<'t, V> {
        let Some(facts) = self.facts.class(name) else {
            return Err(Failure::fault(FaultKind::UnknownClass(name.clone())));
        };
        let facts = facts.clone();
        if slot_args.len() != facts.generic_params.len() {
            return Err(Failure::fault(FaultKind::ClassArity {
                class: name.clone(),
                declared: facts.generic_params.len(),
                stated: slot_args.len(),
            }));
        }
        // What the slot fixes of each argument: nothing where it mentions a
        // variable the call left open.
        let fixed: Vec<Option<SlotTy>> = slot_args
            .iter()
            .map(|arg| (!mentions_type_var(arg)).then(|| arg.clone()))
            .collect();
        match &tree.shape {
            Shape::Instance {
                class,
                type_args,
                fields,
            } => {
                if class != name {
                    return Err(mismatch(tree, slot));
                }
                let fixed = match type_args {
                    // The value knows its arguments: they have to be the
                    // slot's wherever the slot fixes one.
                    Some(args) => {
                        let args = self.host_type_args(&facts, args)?;
                        let agree = args.iter().zip(&fixed).all(|(arg, fixed)| {
                            fixed.as_ref().is_none_or(|fixed| equivalent(fixed, arg))
                        });
                        if !agree {
                            return Err(mismatch(tree, slot));
                        }
                        args.into_iter().map(Some).collect()
                    }
                    None => fixed,
                };
                let present: Vec<(String, &'t Tree<V>)> = fields
                    .iter()
                    .map(|(name, field)| (name.clone(), field))
                    .collect();
                self.fields(&facts, &fixed, &present, FieldSource::Instance)
            }
            Shape::Map(entries) => {
                let mut fields = Vec::with_capacity(entries.len());
                for (key, item) in entries {
                    let Shape::Scalar(Scalar::String) = key.shape else {
                        return Err(Failure::unfit(UnfitKind::MapKey {
                            expected: SlotTy::String,
                            found: key,
                        })
                        .at(Step::Key(key)));
                    };
                    let text = key.value.text().map_err(Failure::host)?;
                    fields.push((text.into_owned(), item));
                }
                self.fields(&facts, &fixed, &fields, FieldSource::Mapping)
            }
            _ => Err(mismatch(tree, slot)),
        }
    }

    /// A class's fields against their declared types, with `fixed` the type
    /// argument at each position that is already known. An argument that is
    /// not is solved from the fields: they are planned with it open, it is
    /// bound by the types they have, and they are planned again against the
    /// solution so each states the type the class gives it.
    fn fields(
        &mut self,
        class: &crate::facts::ClassFacts,
        fixed: &[Option<SlotTy>],
        fields: &[(String, &'t Tree<V>)],
        source: FieldSource,
    ) -> Planned<'t, V> {
        debug_assert_eq!(fixed.len(), class.generic_params.len());
        if let FieldSource::Mapping = source
            && let Some((unknown, _)) = fields
                .iter()
                .find(|(name, _)| !class.fields.iter().any(|(declared, _)| declared == name))
        {
            return Err(Failure::unfit(UnfitKind::UnknownField {
                class: class.name.clone(),
                field: bounded(unknown.clone()),
            }));
        }
        let args: Vec<SlotTy> = if fixed.iter().all(Option::is_some) {
            fixed.iter().flatten().cloned().collect()
        } else {
            let known = |param: &ParamTy| {
                usize::try_from(param.index())
                    .ok()
                    .and_then(|index| fixed.get(index).cloned().flatten())
            };
            let mut solved = Solution::new(class.generic_params.len());
            for (name, declared) in &class.fields {
                let Some((_, value)) = fields.iter().find(|(field, _)| field == name) else {
                    continue;
                };
                let template = substitute(declared, &known);
                let found = self
                    .plan(value, &template)
                    .map_err(|failure| failure.at(Step::Field(name.clone())))?
                    .ty();
                solved.learn(&template, &found);
            }
            let mut args = Vec::with_capacity(class.generic_params.len());
            for (index, (fixed, param)) in fixed.iter().zip(&class.generic_params).enumerate() {
                if let Some(fixed) = fixed {
                    args.push(fixed.clone());
                    continue;
                }
                if solved.conflicts(index) {
                    return Err(Failure::unfit(UnfitKind::TypeArgConflict {
                        class: class.name.clone(),
                        param: param.as_str().to_string(),
                    }));
                }
                match solved.binding(index) {
                    Some(ty) if !has_open(&ty) => args.push(ty),
                    Some(_) | None => {
                        return Err(Failure::unfit(UnfitKind::MissingTypeArgs(
                            class.name.clone(),
                        )));
                    }
                }
            }
            args
        };
        let resolve = |param: &ParamTy| {
            usize::try_from(param.index())
                .ok()
                .filter(|index| *index < class.generic_params.len())
                .and_then(|index| args.get(index).cloned())
        };
        let mut nodes = Vec::with_capacity(class.fields.len());
        for (name, declared) in &class.fields {
            let slot = substitute(declared, &resolve);
            let node = match fields.iter().find(|(field, _)| field == name) {
                Some((_, value)) => self
                    .plan(value, &slot)
                    .map_err(|failure| failure.at(Step::Field(name.clone())))?,
                None => {
                    // A field left out is null where the class allows null.
                    let nullable = match self.facts.unfold(&slot) {
                        SlotTy::Null => true,
                        SlotTy::Union(members) => {
                            self.facts.flatten(members).iter().any(|m| m.is_null())
                        }
                        _ => false,
                    };
                    if nullable {
                        Rc::new(Node::Null)
                    } else {
                        return Err(Failure::unfit(UnfitKind::MissingField {
                            class: class.name.clone(),
                            field: name.clone(),
                        })
                        .at(Step::Field(name.clone())));
                    }
                }
            };
            nodes.push((name.clone(), node));
        }
        Ok(Rc::new(Node::Class {
            ty: SlotTy::Class(WireHead::Static(class.name.clone()), args.into()),
            fields: nodes,
        }))
    }

    // ── Callables and leaves ────────────────────────────────────────────────

    fn callable(tree: &'t Tree<V>, slot: &SlotTy) -> Planned<'t, V> {
        if !matches!(tree.shape, Shape::Callable) {
            return Err(mismatch(tree, slot));
        }
        // A callable's type is its slot's: it has none of its own to bind a
        // variable from, so one the slot leaves open is a hole.
        Ok(Rc::new(Node::Callable {
            value: tree,
            signature: holes_for_vars(&callable_signature(slot)),
        }))
    }

    /// A leaf in `slot`: a handle is judged live by the engine and fits any
    /// slot; the others fit the slot of their kind, or one that fixes nothing.
    fn leaf_in(&mut self, tree: &'t Tree<V>, kind: LeafKind, slot: &SlotTy) -> Planned<'t, V> {
        if let SlotTy::Union(members) = slot {
            let members = self.facts.flatten(members);
            return self.union(tree, &members);
        }
        if self.rank(tree, slot).is_some() {
            Ok(Rc::new(Node::Leaf { value: tree, kind }))
        } else {
            Err(mismatch(tree, slot))
        }
    }
}

/// Where a class's fields come from, which decides the rules for a field the
/// value does not have.
#[derive(Debug, Clone, Copy)]
enum FieldSource {
    Instance,
    Mapping,
}

// ── Solving type variables from values ──────────────────────────────────────

/// What one declaration's type variable is bound to so far.
#[derive(Debug, Clone, Default)]
enum Binding {
    /// Nothing has said.
    #[default]
    Unbound,
    /// A value under a union arrived and a known sibling took it (`null`
    /// under `T | null`, `"hi"` under `T | string`): nothing pins the
    /// variable, so it is `never` unless some other value binds it.
    Absorbed,
    Bound(SlotTy),
    /// Two values disagree; the first is kept so a message can name it.
    Conflict(SlotTy),
}

/// What a container's elements or a class's fields say its type variables
/// are. A type with a hole (an empty container's) is evidence only until a
/// value with elements binds the variable, and never conflicts with one; an
/// opaque type (a handle's) is kept as it is, so the consumer refuses it.
#[derive(Debug)]
struct Solution {
    bindings: Vec<Binding>,
}

impl Solution {
    /// A solution for a declaration with `arity` type parameters.
    fn new(arity: usize) -> Self {
        Solution {
            bindings: vec![Binding::Unbound; arity],
        }
    }

    /// A solution for the variables `template` mentions: sized by the
    /// template, which comes from the program's facts, never from the host.
    fn for_template(template: &SlotTy) -> Self {
        let mut arity = 0;
        visit_vars(template, &mut |param| {
            if let Ok(index) = usize::try_from(param.index())
                && !is_sentinel(param)
            {
                arity = arity.max(index + 1);
            }
        });
        Self::new(arity)
    }

    /// Learn from a value of type `found` under `template`.
    fn learn(&mut self, template: &SlotTy, found: &SlotTy) {
        match template {
            SlotTy::TypeVar(param) => {
                let Some(binding) = self.slot(param) else {
                    return;
                };
                *binding = match std::mem::take(binding) {
                    Binding::Unbound | Binding::Absorbed => Binding::Bound(found.clone()),
                    Binding::Bound(bound) if equivalent(&bound, found) => Binding::Bound(bound),
                    Binding::Bound(bound) if has_open(&bound) && !has_open(found) => {
                        Binding::Bound(found.clone())
                    }
                    Binding::Bound(bound) if has_open(found) => Binding::Bound(bound),
                    Binding::Bound(bound) | Binding::Conflict(bound) => Binding::Conflict(bound),
                };
            }
            SlotTy::List(inner) => {
                if let SlotTy::List(found) = found {
                    self.learn(inner, found);
                }
            }
            SlotTy::Map { key, value } => {
                if let SlotTy::Map {
                    key: found_key,
                    value: found_value,
                } = found
                {
                    self.learn(key, found_key);
                    self.learn(value, found_value);
                }
            }
            SlotTy::Class(name, args) => {
                if let SlotTy::Class(found_name, found_args) = found
                    && found_name == name
                    && found_args.len() == args.len()
                {
                    for (arg, found) in args.iter().zip(found_args.iter()) {
                        self.learn(arg, found);
                    }
                }
            }
            // `T | X` learns `T` from whatever of the value's type is not an
            // `X`: `int` from `int`, and from `int | null` under `T | null`;
            // `Box<T> | null` learns through the one open member. A value a
            // known member took leaves the open member's variables absorbed.
            SlotTy::Union(members) => {
                let (open, fixed): (Vec<&SlotTy>, Vec<&SlotTy>) =
                    members.iter().partition(|member| mentions_type_var(member));
                let [open] = open[..] else {
                    return;
                };
                let rest: Vec<SlotTy> = match found {
                    SlotTy::Union(found) => found
                        .iter()
                        .filter(|member| !fixed.iter().any(|fixed| equivalent(fixed, member)))
                        .cloned()
                        .collect(),
                    other if fixed.iter().any(|fixed| equivalent(fixed, other)) => Vec::new(),
                    other => vec![other.clone()],
                };
                match rest.len() {
                    0 => self.absorb(open),
                    1 => self.learn(open, &rest[0]),
                    _ => self.learn(open, &SlotTy::Union(rest.into())),
                }
            }
            SlotTy::Function {
                params,
                ret,
                throws,
            } => {
                if let SlotTy::Function {
                    params: found_params,
                    ret: found_ret,
                    throws: found_throws,
                } = found
                    && params.len() == found_params.len()
                {
                    for (param, found) in params.iter().zip(found_params.iter()) {
                        self.learn(&param.ty, &found.ty);
                    }
                    self.learn(ret, found_ret);
                    self.learn(throws, found_throws);
                }
            }
            SlotTy::Future(value, error) => {
                if let SlotTy::Future(found_value, found_error) = found {
                    self.learn(value, found_value);
                    self.learn(error, found_error);
                }
            }
            SlotTy::Int
            | SlotTy::Bigint
            | SlotTy::Float
            | SlotTy::String
            | SlotTy::Bool
            | SlotTy::Null
            | SlotTy::Uint8Array
            | SlotTy::Media(_)
            | SlotTy::Literal(..)
            | SlotTy::Enum(_)
            | SlotTy::EnumVariant(..)
            | SlotTy::TypeAlias(_)
            | SlotTy::Interface(..)
            | SlotTy::AssociatedTypeProjection { .. }
            | SlotTy::RustType
            | SlotTy::Type
            | SlotTy::Resource
            | SlotTy::PromptAst
            | SlotTy::Unknown
            | SlotTy::Never => {}
        }
    }

    /// A known member took the value: every variable of `open` that nothing
    /// else binds is `never`.
    fn absorb(&mut self, open: &SlotTy) {
        visit_vars(open, &mut |param| {
            if let Some(binding) = self.slot(param)
                && matches!(binding, Binding::Unbound)
            {
                *binding = Binding::Absorbed;
            }
        });
    }

    /// The binding of a declared variable; `None` for a sentinel or an index
    /// the declaration does not have.
    fn slot(&mut self, param: &ParamTy) -> Option<&mut Binding> {
        if is_sentinel(param) {
            return None;
        }
        usize::try_from(param.index())
            .ok()
            .and_then(|index| self.bindings.get_mut(index))
    }

    fn binding(&self, index: usize) -> Option<SlotTy> {
        match self.bindings.get(index)? {
            Binding::Unbound => None,
            Binding::Absorbed => Some(SlotTy::Never),
            Binding::Bound(ty) | Binding::Conflict(ty) => Some(ty.clone()),
        }
    }

    fn conflicts(&self, index: usize) -> bool {
        matches!(self.bindings.get(index), Some(Binding::Conflict(_)))
    }

    /// `template` with every solved variable substituted.
    fn apply(&self, template: &SlotTy) -> SlotTy {
        substitute(template, &|param| {
            usize::try_from(param.index())
                .ok()
                .and_then(|index| self.binding(index))
        })
    }
}

/// Every type variable `ty` mentions, in order of appearance.
fn visit_vars(ty: &SlotTy, f: &mut impl FnMut(&ParamTy)) {
    if let SlotTy::TypeVar(param) = ty {
        f(param);
    }
    ty.visit_children(&mut |child| visit_vars(child, f));
}

// ── Holes and opaque types: where a value gives no evidence ─────────────────

/// The index of the anonymous type variable a hole is spelled as, and of the
/// one an opaque type is spelled as: indexes no declaration has, so nothing
/// resolves or binds them.
const HOLE_INDEX: u32 = u32::MAX;
const OPAQUE_INDEX: u32 = u32::MAX - 1;

fn is_sentinel(param: &ParamTy) -> bool {
    param.index() == HOLE_INDEX || param.index() == OPAQUE_INDEX
}

/// A type the value gives no evidence of: an empty container's element type,
/// a type variable nothing bound. A sibling with elements fills it. An
/// anonymous type variable until something does; `never` would do, but
/// `throws never` is a type a value has.
fn hole() -> SlotTy {
    SlotTy::TypeVar(ParamTy::new(HOLE_INDEX, Name::new("_")))
}

/// A type the value has but the host cannot see: a handle's. No sibling
/// fills it; under `unknown` it is `unknown`, under an open type variable the
/// call is refused.
fn opaque() -> SlotTy {
    SlotTy::TypeVar(ParamTy::new(OPAQUE_INDEX, Name::new("?")))
}

fn is_hole(ty: &SlotTy) -> bool {
    matches!(ty, SlotTy::TypeVar(param) if param.index() == HOLE_INDEX)
}

fn is_open(ty: &SlotTy) -> bool {
    matches!(ty, SlotTy::TypeVar(param) if is_sentinel(param))
}

/// Does `ty` mention a hole or an opaque type anywhere?
fn has_open(ty: &SlotTy) -> bool {
    if is_open(ty) {
        return true;
    }
    let mut found = false;
    ty.visit_children(&mut |child| found = found || has_open(child));
    found
}

/// `ty` with every hole and opaque type filled by `with`. A union one of
/// whose members is `unknown` is `unknown`: there is no narrower type to
/// state.
fn fill(ty: &SlotTy, with: &SlotTy) -> SlotTy {
    if is_open(ty) {
        return with.clone();
    }
    match ty.map_children(&mut |child| fill(child, with)) {
        SlotTy::Union(members) if members.contains(&SlotTy::Unknown) => SlotTy::Unknown,
        other => other,
    }
}

/// `ty` with every type variable a hole: what a slot's variable is once the
/// value under the slot has had its say and bound nothing.
fn holes_for_vars(ty: &SlotTy) -> SlotTy {
    substitute(ty, &|_| Some(hole()))
}

/// Would `holey`, with its holes filled, be `target`? A hole takes anything,
/// an opaque type nothing; elsewhere the two have to agree.
fn fills(holey: &SlotTy, target: &SlotTy) -> bool {
    if is_hole(holey) {
        return true;
    }
    match (holey, target) {
        (SlotTy::List(holey), SlotTy::List(target)) => fills(holey, target),
        (
            SlotTy::Map {
                key: holey_key,
                value: holey_value,
            },
            SlotTy::Map { key, value },
        ) => fills(holey_key, key) && fills(holey_value, value),
        (SlotTy::Class(holey_name, holey_args), SlotTy::Class(name, args)) => {
            holey_name == name
                && holey_args.len() == args.len()
                && holey_args.iter().zip(args.iter()).all(|(h, t)| fills(h, t))
        }
        // A hole sits in a list's element, a map's value or a callable's
        // signature; a signature's holes are anonymous and no sibling fills
        // them, so anything else has to be the same type.
        _ => equivalent(holey, target),
    }
}

/// `node` stated under `target`: `target` itself when it has the node's shape
/// (an empty list inside an `int[][]` states `int[]`), the member the node
/// inhabits when `target` is a union, and otherwise (an `unknown` target, a
/// union with no member of its shape) the node's own type with `unknown` for
/// what stays open. The statement is pushed down to every child whose type
/// has a hole. A node that states no type, or whose type is already closed,
/// is itself.
fn restate<V>(node: Rc<Node<'_, V>>, target: SlotTy) -> Rc<Node<'_, V>> {
    let own = node.ty();
    if !has_open(&own) {
        return node;
    }
    let ty = if fills(&own, &target) {
        target
    } else if let SlotTy::Union(members) = &target
        && let Some(member) = members.iter().find(|member| fills(&own, member))
    {
        member.clone()
    } else {
        fill(&own, &SlotTy::Unknown)
    };
    match (&*node, &ty) {
        (Node::List { items, .. }, SlotTy::List(inner)) => {
            let items = items
                .iter()
                .map(|item| restate(Rc::clone(item), (**inner).clone()))
                .collect();
            Rc::new(Node::List { ty, items })
        }
        (Node::Map { entries, .. }, SlotTy::Map { value, .. }) => {
            let entries = entries
                .iter()
                .map(|(key, item)| {
                    let key = match key {
                        KeyNode::String(tree) => KeyNode::String(tree),
                    };
                    (key, restate(Rc::clone(item), (**value).clone()))
                })
                .collect();
            Rc::new(Node::Map { ty, entries })
        }
        (Node::Callable { value, .. }, _) => Rc::new(Node::Callable {
            value,
            signature: ty,
        }),
        // `ty` fills `own`, so it has the node's shape; a list or map here is
        // stated as its own type with `unknown` for what stays open, which
        // does (`fill` keeps the shape).
        (Node::List { .. } | Node::Map { .. }, _) => {
            debug_assert!(false, "a {own} restated as {ty}");
            restate(node, fill(&own, &SlotTy::Unknown))
        }
        // A class's holes are filled where it is planned; a scalar, variant
        // or leaf states nothing.
        (
            Node::Class { .. }
            | Node::Null
            | Node::Scalar { .. }
            | Node::EnumVariant { .. }
            | Node::Leaf { .. },
            _,
        ) => node,
    }
}

/// The element type of a natural container, from the types its elements
/// have: the one they share, their union otherwise, a hole for none. An
/// element whose type has a hole takes the type a sibling has when it can
/// (`[[], [1]]` is `int[][]`) and is restated to it; what no sibling fills
/// stays as it is, for the slot above to fill or refuse.
fn join<V>(nodes: &mut [Rc<Node<'_, V>>]) -> SlotTy {
    let tys: Vec<SlotTy> = nodes.iter().map(|node| node.ty()).collect();
    let (element, restated) = merge(&tys);
    for (node, ty) in nodes.iter_mut().zip(restated) {
        if let Some(ty) = ty {
            *node = restate(Rc::clone(node), ty);
        }
    }
    element
}

/// What the elements of one container say their element type is, as evidence
/// for the slot's variables: the one type they share, their union otherwise
/// (`[1, "a"]` under `T[]` binds `T = int | string`, as the compiler types the
/// literal). An element with a hole adds nothing the others already say.
fn evidence(found: &[SlotTy]) -> SlotTy {
    merge(found).0
}

/// The one type `tys` make together, and for each of them the type it is
/// restated to when a sibling fills its holes.
fn merge(tys: &[SlotTy]) -> (SlotTy, Vec<Option<SlotTy>>) {
    let mut members: Vec<SlotTy> = Vec::new();
    for ty in tys {
        if !has_open(ty) {
            push_distinct(&mut members, ty.clone());
        }
    }
    let mut restated = vec![None; tys.len()];
    for (index, ty) in tys.iter().enumerate() {
        if !has_open(ty) {
            continue;
        }
        match members.iter().find(|target| fills(ty, target)) {
            Some(target) => restated[index] = Some(target.clone()),
            None => push_distinct(&mut members, ty.clone()),
        }
    }
    let element = if members.contains(&SlotTy::Unknown) {
        SlotTy::Unknown
    } else {
        match members.len() {
            0 => hole(),
            1 => members.pop().expect("one element"),
            _ => SlotTy::Union(members.into()),
        }
    };
    (element, restated)
}

// ── Type identity ───────────────────────────────────────────────────────────

/// `ty` with every union's members in one order and without duplicates, so
/// two spellings of one type compare equal: a union's member order is never
/// an identity.
fn canonical(ty: &SlotTy) -> SlotTy {
    match ty.map_children(&mut canonical) {
        SlotTy::Union(members) => {
            let mut distinct: Vec<SlotTy> = Vec::with_capacity(members.len());
            for member in &members {
                if !distinct.contains(member) {
                    distinct.push(member.clone());
                }
            }
            distinct.sort_by_cached_key(ToString::to_string);
            match distinct.len() {
                1 => distinct.pop().expect("one member"),
                _ => SlotTy::Union(distinct.into()),
            }
        }
        other => other,
    }
}

/// Are `a` and `b` the same type?
fn equivalent(a: &SlotTy, b: &SlotTy) -> bool {
    a == b || canonical(a) == canonical(b)
}

fn push_distinct(tys: &mut Vec<SlotTy>, ty: SlotTy) {
    if !tys.iter().any(|seen| equivalent(seen, &ty)) {
        tys.push(ty);
    }
}

fn mismatch<'t, V: HostValue>(tree: &'t Tree<V>, slot: &SlotTy) -> Box<Failure<'t, V>> {
    Failure::unfit(UnfitKind::Mismatch {
        expected: slot.clone(),
        found: tree,
    })
}

/// How a scalar stands to a member: by the language's rule for its slot.
fn scalar_rank<V: HostValue>(scalar: &Scalar, member: &SlotTy) -> Option<Rank> {
    let kind = match member {
        SlotTy::Int => ScalarSlot::Int,
        SlotTy::Bigint => ScalarSlot::Bigint,
        SlotTy::Float => ScalarSlot::Float,
        SlotTy::Bool => ScalarSlot::Bool,
        SlotTy::String => ScalarSlot::String,
        SlotTy::Uint8Array => ScalarSlot::Bytes,
        SlotTy::Literal(literal, _) => match literal {
            Literal::Int(_) => ScalarSlot::Int,
            Literal::Bigint(_) => ScalarSlot::Bigint,
            Literal::Float(_) => return None,
            Literal::String(_) => ScalarSlot::String,
            Literal::Bool(_) => ScalarSlot::Bool,
        },
        // A string names a variant, by conversion.
        SlotTy::Enum(_) | SlotTy::EnumVariant(..) => {
            return matches!(scalar, Scalar::String).then_some(Rank::Conversion);
        }
        _ => return None,
    };
    match V::RULES.fit(scalar, kind) {
        Answer::AsIs(_) => Some(Rank::Identity),
        Answer::Converted(_) => Some(Rank::Conversion),
        Answer::No => None,
    }
}

fn scalar_ty(scalar: &Scalar) -> SlotTy {
    match scalar {
        Scalar::Int(_) => SlotTy::Int,
        Scalar::BigInt(_) => SlotTy::Bigint,
        Scalar::Float(_) => SlotTy::Float,
        Scalar::Bool(_) => SlotTy::Bool,
        Scalar::String => SlotTy::String,
        Scalar::Bytes => SlotTy::Uint8Array,
    }
}

/// `ai.Prompt` is the class a rendered prompt inhabits; a program declares it
/// as a class, the wire carries the prompt itself.
fn is_prompt_class(name: &Head) -> bool {
    name.render_dotted(false) == <SlotTy>::PromptAst.to_string()
}

/// The function type a host callable is bound to: the slot's, with a throws
/// clause that mentions a type variable stated as `unknown`. Such a clause is
/// an effect parameter the caller never binds (`throws E`, `throws E | Err`),
/// and a host callable may throw anything.
pub(crate) fn callable_signature(slot: &SlotTy) -> SlotTy {
    match slot {
        SlotTy::Function {
            params,
            ret,
            throws,
        } if mentions_type_var(throws) => SlotTy::Function {
            params: params.clone(),
            ret: ret.clone(),
            throws: Box::new(SlotTy::Unknown),
        },
        other => other.clone(),
    }
}
