//! μ-canonicalization: the automaton stage that makes canonical forms unique
//! representatives of the **equirecursive** equivalence class.
//!
//! A recursive type is a regular tree; a canonical `NormalTy` with μ-binders is
//! one finite spelling of it, and different spellings of the same tree (alias
//! renamings, partial unfoldings, mutually recursive definitions) differ
//! structurally. This module quotients those differences away, as a typestate
//! pipeline — each stage's invariant is carried by its type:
//!
//! [`Builder`] → [`Built`] → [`Closed`] → [`Minimal`] → read-back / render
//!
//! 1. **Build** ([`Builder::build`]) — intern the term as a term-graph
//!    automaton: one state per node, a μ-binder is the knot (its state *is* its
//!    body's state, carrying the alias name), a recursion variable is a
//!    back-edge.
//! 2. **ε-closure** ([`Built::epsilon_close`]) — a union member that is itself a
//!    union state is an ε-edge; per ε-SCC, members become the
//!    constructor-successors of the component. This is union flattening
//!    generalized through recursion, and it implements the productivity ruling
//!    for unguarded self-references: a member reachable only through ε-cycles
//!    contributes nothing (`type A = A | A[]` ≡ `μX.X[]`), and a
//!    constructor-free ε-component is uninhabited (`μX.X` ≡ `never`). After
//!    this step every μ body is constructor-headed (contractive) — the
//!    precondition of the assumption-set subtype algorithm.
//! 3. **Minimization + per-state algebra** ([`Closed::minimize_and_absorb`]) —
//!    Moore-style partition refinement merges bisimilar states (equal unravelled
//!    trees — every spelling of one tree at any unfolding depth), interleaved to
//!    a fixpoint with the union algebra per state: members *materialized as
//!    closed read-backs* feed the real subtype checker (full context), so the
//!    absorptions the bottom-up pass deferred for open members complete here,
//!    and completeness collapses apply across merged members.
//! 4. **Read-back / render** ([`Minimal::read_back`], [`Minimal::render_root`])
//!    — a deterministic DFS emits the canonical μ-term: a back edge to an
//!    on-path state becomes a de Bruijn `RecVar`, the revisited state is wrapped
//!    in `Mu`, off-path revisits re-expand, union members sort by the canonical
//!    `Ord`. Each emitted binder's [`MuDisplay`] carries the **named-cut**
//!    rendering of its subterm (recursion folded to alias names at named cycle
//!    states), computed here while the automaton — which knows the names — is
//!    in scope.
//!
//! # Representation: borrow the input, compare by id
//!
//! The automaton borrows from the input term (lifetime `'a`) and interns every
//! heavy payload — qualified type names, member names, leaf terms — into dense
//! ids at build time ([`Interner`]). All downstream phases (ε-closure,
//! refinement, absorption bookkeeping) are pure integer work; a string is
//! compared only at its single intern probe, and owned values are cloned only at
//! output emission. The id seam is also where a future runtime identity handle
//! (a heap pointer standing for a class/interface/enum) slots in: only the
//! intern boundary knows what a [`NameId`] refers to.
//!
//! # Determinism
//!
//! The result must be a function of the input term alone. Every collection whose
//! iteration order can reach the output is a `Vec`, `BTreeSet`, or `BTreeMap` —
//! never a hash map — state ids follow the input walk, and intern ids follow
//! first encounter. Any hash-iteration order feeding the output is a bug.
//!
//! # Termination
//!
//! ε-closure and refinement are fixpoint passes over finitely many states. The
//! canonicalization loop only merges states, removes union members, redirects to
//! existing states, or rewrites a complete variant set to its enum (which
//! nothing can reintroduce) — all monotone — so it reaches a fixpoint.
//!
//! # Names, covers, and the bail
//!
//! At build time every cycle passes through a named state: the only back-edges
//! the named intermediate contains are alias re-encounters, so every
//! cycle-closing edge targets a binder state carrying its alias name; redirects
//! transfer names and merges union them. ε-splicing, however, can *bypass* the
//! named state — splicing named `u_A` (`type A = int | (A | null)[]`) into the
//! inner union leaves the surviving cycle nameless. The renderer therefore cuts
//! cycles two ways: at named cyclic states (`Ty::TypeAlias`), and at **subset
//! covers** — an unnamed cyclic union containing everything a named cyclic
//! union holds renders as `name | extras`, which is exact by union semantics.
//! Absorption is withheld from unnamed cyclic unions so covers survive
//! ([`algebra_pass`]). A cycle neither names nor covers can cut has no finite
//! alias-based `Ty` spelling at all; rather than hand a wrong display to a fact
//! oracle, the renderer **bails** and the pipeline degrades to the sound
//! pre-automaton form (see [`canonicalize_mu`]).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::{Head, MuDisplay, NormalParam, NormalTy, TypeContext};
use crate::{FunctionParamMode, Name, Ty, TyAttr};

/// Canonicalize a term containing at least one μ-binder.
///
/// Falls back to the (already sound, bottom-up-canonicalized) input when the
/// automaton's rendering bails — see [`Renderer`]: an exotic cycle that no
/// alias name or subset cover can cut has no exact `Ty` spelling, and a wrong
/// display must never reach a fact oracle. The fallback degrades only
/// *completeness* (two such spellings may miss an equivalence), never
/// soundness.
pub(super) fn canonicalize_mu<H: Head, C: TypeContext<H>>(
    term: NormalTy<H>,
    ctx: &C,
) -> NormalTy<H> {
    let out = Builder::default()
        .build(&term)
        .epsilon_close()
        .minimize_and_absorb(ctx)
        .read_back();
    out.unwrap_or(term)
}

/// [`canonicalize_mu`] plus the root rendering for `normalize`: root-unfold-once
/// (a recursive alias exposes its head constructor; nested recursion folds to
/// alias names). The bail fallback renders the input term via its interim
/// (legacy) displays — a correct, merely less-canonical spelling.
pub(super) fn canonicalize_mu_with_render<H: Head, C: TypeContext<H>>(
    term: NormalTy<H>,
    ctx: &C,
) -> (NormalTy<H>, Ty<H>) {
    let minimal = Builder::default()
        .build(&term)
        .epsilon_close()
        .minimize_and_absorb(ctx);
    if let (Some(t), Some(r)) = (minimal.read_back(), minimal.render_root()) {
        return (t, r);
    }
    drop(minimal);
    let rendered = term.clone().into_ty();
    (term, rendered)
}

// ═══════════════════════════════════════════════════════════════════════════
// INTERNING
// ═══════════════════════════════════════════════════════════════════════════

/// A qualified type name, interned. Downstream phases compare and store these
/// as integers; the value is recovered only at output emission. (The seam for a
/// future runtime identity handle: only the interner knows the referent.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct NameId(u32);

/// A member/parameter/variant name, interned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct StrId(u32);

/// A childless canonical leaf term, interned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct LeafId(u32);

/// First-encounter interning of borrowed values: one ordered probe per
/// occurrence at build time buys integer comparison everywhere after.
struct Interner<'a, T: ?Sized + Ord> {
    ids: BTreeMap<&'a T, u32>,
    values: Vec<&'a T>,
}

impl<'a, T: ?Sized + Ord> Default for Interner<'a, T> {
    fn default() -> Self {
        Self {
            ids: BTreeMap::new(),
            values: Vec::new(),
        }
    }
}

impl<'a, T: ?Sized + Ord> Interner<'a, T> {
    fn intern(&mut self, value: &'a T) -> u32 {
        if let Some(&id) = self.ids.get(value) {
            return id;
        }
        let id = self.values.len() as u32;
        self.ids.insert(value, id);
        self.values.push(value);
        id
    }

    fn get(&self, id: u32) -> &'a T {
        self.values[id as usize]
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// AUTOMATON CORE
// ═══════════════════════════════════════════════════════════════════════════

type StateId = usize;

/// One term-graph node. Children are state ids (resolve through
/// [`Automaton::resolve`] before use); payloads are interned ids. `Leaf` holds
/// the childless canonical variants *except* enums, which get their own
/// id-based kinds so the enum-completeness collapse can synthesize an enum
/// state without owning a term.
enum Node {
    Leaf(LeafId),
    Enum(NameId),
    EnumVariant(NameId, StrId),
    Class(NameId, Vec<StateId>),
    Interface(NameId, Vec<StateId>, Vec<(StrId, StateId)>),
    List(StateId),
    Map(StateId, StateId),
    Future(StateId, StateId),
    Union(Vec<StateId>),
    Function {
        params: Vec<(Option<StrId>, FunctionParamMode, StateId)>,
        ret: StateId,
        throws: StateId,
    },
    Projection {
        base: StateId,
        interface: StateId,
        member: StrId,
    },
}

struct State {
    node: Node,
    /// Alias names denoting this state (μ-binder origins; unioned on merge,
    /// transferred on redirect). Ordered by id — ties in rendering are broken by
    /// the *name* ordering below, not id order.
    names: BTreeSet<NameId>,
}

/// The special leaves, pre-interned by [`Automaton::new`] so the hot
/// special-case probes are id compares.
///
/// Functions wrapping an inline `const` block, not `const` items: a `const` item
/// cannot name an enclosing generic parameter, and `&SOME_CONST` of a generic
/// type will not promote to `'static` either — rustc cannot prove
/// `NormalTy` is drop-free for an arbitrary `H`. An inline `const` block
/// *does* inherit the generics and *does* promote, which is what lets these stay
/// `'static` and keeps [`Interner`] borrowing rather than cloning.
fn never<H: Head>() -> &'static NormalTy<H> {
    &const { NormalTy::Never }
}

fn unknown_top<H: Head>() -> &'static NormalTy<H> {
    &const { NormalTy::BuiltinUnknown }
}

fn bool_leaf<H: Head>() -> &'static NormalTy<H> {
    &const { NormalTy::Bool }
}

fn lit_true<H: Head>() -> &'static NormalTy<H> {
    &const { NormalTy::Literal(crate::Literal::Bool(true)) }
}

fn lit_false<H: Head>() -> &'static NormalTy<H> {
    &const { NormalTy::Literal(crate::Literal::Bool(false)) }
}

struct Automaton<'a, H: Head> {
    states: Vec<State>,
    /// Redirect chains from ε singleton collapses, minimization merges, and
    /// binder knot-tying; [`Self::resolve`] follows them to the live state.
    redirect: Vec<StateId>,
    names: Interner<'a, H>,
    strs: Interner<'a, Name>,
    leaves: Interner<'a, NormalTy<H>>,
    /// Pre-interned specials, so the hot special-case probes are id compares.
    never: LeafId,
    unknown_top: LeafId,
    bool_leaf: LeafId,
    lit_true: LeafId,
    lit_false: LeafId,
}

impl<'a, H: Head> Automaton<'a, H> {
    fn new() -> Self {
        let mut leaves = Interner::default();
        let never = LeafId(leaves.intern(never()));
        let unknown_top = LeafId(leaves.intern(unknown_top()));
        let bool_leaf = LeafId(leaves.intern(bool_leaf()));
        let lit_true = LeafId(leaves.intern(lit_true()));
        let lit_false = LeafId(leaves.intern(lit_false()));
        Self {
            states: Vec::new(),
            redirect: Vec::new(),
            names: Interner::default(),
            strs: Interner::default(),
            leaves,
            never,
            unknown_top,
            bool_leaf,
            lit_true,
            lit_false,
        }
    }

    fn alloc(&mut self, node: Node) -> StateId {
        let id = self.states.len();
        self.states.push(State {
            node,
            names: BTreeSet::new(),
        });
        self.redirect.push(id);
        id
    }

    fn resolve(&self, mut s: StateId) -> StateId {
        while self.redirect[s] != s {
            s = self.redirect[s];
        }
        s
    }

    /// Redirect `from` to `to`, transferring names.
    fn redirect_to(&mut self, from: StateId, to: StateId) {
        debug_assert_ne!(self.resolve(from), self.resolve(to));
        let names = std::mem::take(&mut self.states[from].names);
        let to = self.resolve(to);
        self.states[to].names.extend(names);
        self.redirect[from] = to;
    }

    fn node(&self, s: StateId) -> &Node {
        &self.states[self.resolve(s)].node
    }

    fn is_never(&self, s: StateId) -> bool {
        matches!(self.node(s), Node::Leaf(l) if *l == self.never)
    }

    fn is_unknown_top(&self, s: StateId) -> bool {
        matches!(self.node(s), Node::Leaf(l) if *l == self.unknown_top)
    }

    /// The lexicographically least alias name of `s`, if any — the
    /// deterministic representative for rendering. (State name sets are keyed
    /// by id; representative selection orders by the *names*.)
    fn representative_name(&self, s: StateId) -> Option<&'a H> {
        self.states[self.resolve(s)]
            .names
            .iter()
            .map(|&id| self.names.get(id.0))
            .min()
    }

    /// All states reachable from `root` (resolved ids, ascending — the
    /// deterministic processing order).
    fn reachable(&self, root: StateId) -> Vec<StateId> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![self.resolve(root)];
        while let Some(s) = stack.pop() {
            if !seen.insert(s) {
                continue;
            }
            for c in children(self.node(s)) {
                stack.push(self.resolve(c));
            }
        }
        seen.into_iter().collect()
    }
}

/// The child state ids of a node, in canonical child order.
fn children(node: &Node) -> Vec<StateId> {
    match node {
        Node::Leaf(_) | Node::Enum(_) | Node::EnumVariant(..) => Vec::new(),
        Node::Class(_, args) => args.clone(),
        Node::Interface(_, args, bindings) => args
            .iter()
            .chain(bindings.iter().map(|(_, s)| s))
            .copied()
            .collect(),
        Node::List(inner) => vec![*inner],
        Node::Map(k, v) | Node::Future(k, v) => vec![*k, *v],
        Node::Union(members) => members.clone(),
        Node::Function {
            params,
            ret,
            throws,
        } => params
            .iter()
            .map(|(_, _, s)| *s)
            .chain([*ret, *throws])
            .collect(),
        Node::Projection {
            base, interface, ..
        } => vec![*base, *interface],
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// STAGE 1: BUILD
// ═══════════════════════════════════════════════════════════════════════════

/// Interns a canonical term into a fresh automaton. Owns the binder stack the
/// walk needs; consumed by [`Builder::build`].
#[derive(Default)]
struct Builder {
    binders: Vec<StateId>,
}

impl Builder {
    fn build<'a, H: Head>(mut self, term: &'a NormalTy<H>) -> Built<'a, H> {
        let mut auto = Automaton::new();
        let root = self.intern_term(&mut auto, term);
        debug_assert!(self.binders.is_empty());
        Built { auto, root }
    }

    /// A μ-binder ties the knot: its reserved state either receives the body's
    /// node (when the body is a direct self-reference this is the ε-self-loop
    /// `Union([s])`, resolved to `never` by closure) or redirects to the body's
    /// state, carrying the alias name either way. A recursion variable is an
    /// edge to the binder state `index` levels up the stack.
    fn intern_term<'a, H: Head>(
        &mut self,
        auto: &mut Automaton<'a, H>,
        term: &'a NormalTy<H>,
    ) -> StateId {
        match term {
            NormalTy::Mu { binder, body } => {
                let s = auto.alloc(Node::Union(Vec::new()));
                if let Some(name) = &binder.name {
                    let id = NameId(auto.names.intern(name));
                    auto.states[s].names.insert(id);
                }
                self.binders.push(s);
                let b = self.intern_term(auto, body);
                self.binders.pop();
                if auto.resolve(b) == s {
                    // `μX.X`: the body is the back-reference itself — an
                    // ε-self-loop (constructor-free, hence `never` after closure).
                    auto.states[s].node = Node::Union(vec![s]);
                    s
                } else {
                    auto.redirect_to(s, b);
                    auto.resolve(b)
                }
            }
            NormalTy::RecVar(index) => {
                let i = *index as usize;
                debug_assert!(i < self.binders.len(), "free RecVar reached the automaton");
                self.binders[self.binders.len() - 1 - i]
            }
            NormalTy::Enum(qn) => {
                let qn = NameId(auto.names.intern(qn));
                auto.alloc(Node::Enum(qn))
            }
            NormalTy::EnumVariant(qn, v) => {
                let qn = NameId(auto.names.intern(qn));
                let v = StrId(auto.strs.intern(v));
                auto.alloc(Node::EnumVariant(qn, v))
            }
            NormalTy::Class(qn, args) => {
                let qn = NameId(auto.names.intern(qn));
                let args = args.iter().map(|a| self.intern_term(auto, a)).collect();
                auto.alloc(Node::Class(qn, args))
            }
            NormalTy::Interface(qn, args, bindings) => {
                let qn = NameId(auto.names.intern(qn));
                let args = args.iter().map(|a| self.intern_term(auto, a)).collect();
                let bindings = bindings
                    .iter()
                    .map(|(n, t)| (StrId(auto.strs.intern(n)), self.intern_term(auto, t)))
                    .collect();
                auto.alloc(Node::Interface(qn, args, bindings))
            }
            NormalTy::List(inner) => {
                let inner = self.intern_term(auto, inner);
                auto.alloc(Node::List(inner))
            }
            NormalTy::Map { key, value } => {
                let key = self.intern_term(auto, key);
                let value = self.intern_term(auto, value);
                auto.alloc(Node::Map(key, value))
            }
            NormalTy::Future(value, error) => {
                let value = self.intern_term(auto, value);
                let error = self.intern_term(auto, error);
                auto.alloc(Node::Future(value, error))
            }
            NormalTy::Union(members) => {
                let members = members.iter().map(|m| self.intern_term(auto, m)).collect();
                auto.alloc(Node::Union(members))
            }
            NormalTy::Function {
                params,
                ret,
                throws,
            } => {
                let params = params
                    .iter()
                    .map(|p| {
                        (
                            p.name.as_ref().map(|n| StrId(auto.strs.intern(n))),
                            p.mode,
                            self.intern_term(auto, &p.ty),
                        )
                    })
                    .collect();
                let ret = self.intern_term(auto, ret);
                let throws = self.intern_term(auto, throws);
                auto.alloc(Node::Function {
                    params,
                    ret,
                    throws,
                })
            }
            NormalTy::AssociatedTypeProjection {
                base,
                interface,
                member,
            } => {
                let base = self.intern_term(auto, base);
                let interface = self.intern_term(auto, interface);
                let member = StrId(auto.strs.intern(member));
                auto.alloc(Node::Projection {
                    base,
                    interface,
                    member,
                })
            }
            leaf => {
                let id = LeafId(auto.leaves.intern(leaf));
                auto.alloc(Node::Leaf(id))
            }
        }
    }
}

/// Stage invariant: a faithful term-graph of the input — knots tied (every
/// back-edge targets its binder's state, which carries the alias name), no
/// transformation applied yet.
struct Built<'a, H: Head> {
    auto: Automaton<'a, H>,
    root: StateId,
}

impl<'a, H: Head> Built<'a, H> {
    fn epsilon_close(mut self) -> Closed<'a, H> {
        epsilon_close(&mut self.auto, self.root);
        Closed {
            auto: self.auto,
            root: self.root,
        }
    }
}

/// Stage invariant: ε-closed — no union has a union member, a `never` member,
/// or an unguarded spine; every μ body is constructor-headed (contractive), so
/// the assumption-set subtype algorithm's precondition holds for anything read
/// back from here.
struct Closed<'a, H: Head> {
    auto: Automaton<'a, H>,
    root: StateId,
}

impl<'a, H: Head> Closed<'a, H> {
    /// Interleave partition refinement with the per-state union algebra to a
    /// fixpoint (an absorption redirect can resurface ε-edges, hence the
    /// re-closure inside the loop).
    fn minimize_and_absorb<C: TypeContext<H>>(mut self, ctx: &C) -> Minimal<'a, H> {
        loop {
            minimize(&mut self.auto, self.root);
            if !algebra_pass(&mut self.auto, self.root, ctx) {
                break;
            }
            epsilon_close(&mut self.auto, self.root);
        }
        Minimal {
            auto: self.auto,
            root: self.root,
        }
    }
}

/// Stage invariant: the canonical automaton — coarsest bisimulation, union
/// algebra at fixpoint. Its read-back is the unique canonical μ-term of the
/// input's equivalence class; its renders are the canonical spellings.
struct Minimal<'a, H: Head> {
    auto: Automaton<'a, H>,
    root: StateId,
}

impl<H: Head> Minimal<'_, H> {
    fn read_back(&self) -> Option<NormalTy<H>> {
        read_back(&self.auto, self.root)
    }

    /// The `normalize` rendering of the whole type: a *named* recursive root is
    /// unfolded once (callers rely on `normalize` exposing the head
    /// constructor — impl-subject classification, dispatch-target resolution,
    /// pattern-matrix specialization), with nested recursion folded to alias
    /// names; every other root renders by named-cut directly. `None` when the
    /// renderer bails (see [`Renderer`]).
    fn render_root(&self) -> Option<Ty<H>> {
        let auto = &self.auto;
        let cyclic = cyclic_states(auto, self.root);
        let orders = union_orders(auto, self.root)?;
        let mut renderer = Renderer::new(auto, &cyclic, &orders);
        let r = auto.resolve(self.root);
        let mut path = Vec::new();
        if cyclic.contains(&r) && auto.representative_name(r).is_some() {
            path.push(r);
            let rendered = renderer.structural(r, &mut path);
            path.pop();
            return rendered;
        }
        renderer.render(r, &mut path)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// STAGE 2: ε-CLOSURE
// ═══════════════════════════════════════════════════════════════════════════

/// Close every union state over its ε-edges (members that are themselves union
/// states). Tarjan yields ε-SCCs successors-first, so by the time a component is
/// processed every union it can still reach through ε is already closed; the
/// component's final members are the constructor-successors gathered across it.
fn epsilon_close<H: Head>(auto: &mut Automaton<'_, H>, root: StateId) {
    let sccs = epsilon_sccs(auto, root);
    for scc in sccs {
        let in_scc: BTreeSet<StateId> = scc.iter().copied().collect();
        let mut members: BTreeSet<StateId> = BTreeSet::new();
        for &u in &scc {
            let Node::Union(ms) = auto.node(u) else {
                // A prior component's processing may have redirected this state
                // (nested singleton collapse); nothing left to close.
                continue;
            };
            for m in ms.clone() {
                let r = auto.resolve(m);
                if in_scc.contains(&r) {
                    continue;
                }
                match auto.node(r) {
                    // An already-closed union: splice its (constructor) members.
                    Node::Union(inner) => {
                        let inner = inner.clone();
                        members.extend(inner.into_iter().map(|x| auto.resolve(x)));
                    }
                    _ => {
                        members.insert(r);
                    }
                }
            }
        }
        // `never` contributes nothing; `unknown` absorbs everything.
        members.retain(|&m| !auto.is_never(m));
        let has_top = members.iter().any(|&m| auto.is_unknown_top(m));
        for &u in &scc {
            if auto.resolve(u) != u {
                continue;
            }
            if has_top {
                auto.states[u].node = Node::Leaf(auto.unknown_top);
            } else if members.is_empty() {
                // A constructor-free ε-component: only non-productive circular
                // derivations exist, so nothing inhabits it (`μX.X` ≡ `never`).
                auto.states[u].node = Node::Leaf(auto.never);
            } else if members.len() == 1 {
                let m = *members
                    .first()
                    .unwrap_or_else(|| unreachable!("len checked"));
                if auto.resolve(m) != u {
                    auto.redirect_to(u, m);
                }
            } else {
                auto.states[u].node = Node::Union(members.iter().copied().collect());
            }
        }
    }
}

/// ε-SCCs over union states, in Tarjan completion order (successors first).
fn epsilon_sccs<H: Head>(auto: &Automaton<'_, H>, root: StateId) -> Vec<Vec<StateId>> {
    struct Tarjan<'x, 'a, H: Head> {
        auto: &'x Automaton<'a, H>,
        index: BTreeMap<StateId, usize>,
        low: BTreeMap<StateId, usize>,
        on_stack: BTreeSet<StateId>,
        stack: Vec<StateId>,
        next: usize,
        sccs: Vec<Vec<StateId>>,
    }
    impl<H: Head> Tarjan<'_, '_, H> {
        fn visit(&mut self, s: StateId) {
            self.index.insert(s, self.next);
            self.low.insert(s, self.next);
            self.next += 1;
            self.stack.push(s);
            self.on_stack.insert(s);
            let Node::Union(members) = self.auto.node(s) else {
                unreachable!("ε-SCCs are computed over union states only")
            };
            let eps: Vec<StateId> = members
                .iter()
                .map(|&m| self.auto.resolve(m))
                .filter(|&r| r != s && matches!(self.auto.node(r), Node::Union(_)))
                .collect();
            for t in eps {
                if !self.index.contains_key(&t) {
                    self.visit(t);
                    let l = self.low[&t].min(self.low[&s]);
                    self.low.insert(s, l);
                } else if self.on_stack.contains(&t) {
                    let l = self.index[&t].min(self.low[&s]);
                    self.low.insert(s, l);
                }
            }
            if self.low[&s] == self.index[&s] {
                let mut scc = Vec::new();
                loop {
                    let t = self
                        .stack
                        .pop()
                        .unwrap_or_else(|| unreachable!("Tarjan stack underflow"));
                    self.on_stack.remove(&t);
                    scc.push(t);
                    if t == s {
                        break;
                    }
                }
                scc.sort_unstable();
                self.sccs.push(scc);
            }
        }
    }
    let mut t = Tarjan {
        auto,
        index: BTreeMap::new(),
        low: BTreeMap::new(),
        on_stack: BTreeSet::new(),
        stack: Vec::new(),
        next: 0,
        sccs: Vec::new(),
    };
    for s in auto.reachable(root) {
        if matches!(auto.node(s), Node::Union(_)) && !t.index.contains_key(&s) {
            t.visit(s);
        }
    }
    t.sccs
}

// ═══════════════════════════════════════════════════════════════════════════
// STAGE 3: MINIMIZATION + PER-STATE ALGEBRA
// ═══════════════════════════════════════════════════════════════════════════

/// Moore-style partition refinement to the coarsest bisimulation, then merge
/// each block into its smallest member. Union members refine as a *set* of
/// blocks (duplicates within a block are one member of the tree).
fn minimize<H: Head>(auto: &mut Automaton<'_, H>, root: StateId) {
    let reach = auto.reachable(root);

    // Initial partition by local shape — pure id comparisons.
    let mut block: BTreeMap<StateId, usize> = BTreeMap::new();
    {
        let mut groups: BTreeMap<LocalKey, Vec<StateId>> = BTreeMap::new();
        for &s in &reach {
            groups.entry(local_key(auto, s)).or_default().push(s);
        }
        for (i, states) in groups.into_values().enumerate() {
            for s in states {
                block.insert(s, i);
            }
        }
    }

    // Refine by child blocks until stable.
    loop {
        let mut groups: BTreeMap<(usize, Vec<usize>), Vec<StateId>> = BTreeMap::new();
        for &s in &reach {
            let sig = match auto.node(s) {
                Node::Union(members) => {
                    let mut bs: Vec<usize> =
                        members.iter().map(|&m| block[&auto.resolve(m)]).collect();
                    bs.sort_unstable();
                    bs.dedup();
                    bs
                }
                other => children(other)
                    .into_iter()
                    .map(|c| block[&auto.resolve(c)])
                    .collect(),
            };
            groups.entry((block[&s], sig)).or_default().push(s);
        }
        let stable = groups.len() == block.values().collect::<BTreeSet<_>>().len();
        let mut next: BTreeMap<StateId, usize> = BTreeMap::new();
        for (i, states) in groups.into_values().enumerate() {
            for s in states {
                next.insert(s, i);
            }
        }
        block = next;
        if stable {
            break;
        }
    }

    // Merge each block into its smallest state id.
    let mut rep: BTreeMap<usize, StateId> = BTreeMap::new();
    for &s in &reach {
        rep.entry(block[&s]).or_insert(s); // reach is ascending
    }
    for &s in &reach {
        let r = rep[&block[&s]];
        if s != r {
            auto.redirect_to(s, r);
        }
    }
}

/// The local (childless) shape of a state — everything about a node except its
/// children, as interned ids. Two states can only ever merge if their local
/// keys are equal.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum LocalKey {
    Leaf(LeafId),
    Enum(NameId),
    EnumVariant(NameId, StrId),
    Class(NameId, usize),
    Interface(NameId, usize, Vec<StrId>),
    List,
    Map,
    Future,
    Union,
    Function(Vec<(Option<StrId>, FunctionParamMode)>),
    Projection(StrId),
}

fn local_key<H: Head>(auto: &Automaton<'_, H>, s: StateId) -> LocalKey {
    match auto.node(s) {
        Node::Leaf(l) => LocalKey::Leaf(*l),
        Node::Enum(qn) => LocalKey::Enum(*qn),
        Node::EnumVariant(qn, v) => LocalKey::EnumVariant(*qn, *v),
        Node::Class(qn, args) => LocalKey::Class(*qn, args.len()),
        Node::Interface(qn, args, bindings) => {
            LocalKey::Interface(*qn, args.len(), bindings.iter().map(|(n, _)| *n).collect())
        }
        Node::List(_) => LocalKey::List,
        Node::Map(..) => LocalKey::Map,
        Node::Future(..) => LocalKey::Future,
        Node::Union(_) => LocalKey::Union,
        Node::Function { params, .. } => LocalKey::Function(
            params
                .iter()
                .map(|(name, mode, _)| (*name, *mode))
                .collect(),
        ),
        Node::Projection { member, .. } => LocalKey::Projection(*member),
    }
}

/// Re-run the union algebra per state over the merged automaton: dedup members
/// that became one state, collapse complete enums, and absorb subtype members —
/// with each member materialized as its **closed** read-back so the real subtype
/// checker (full context) decides, completing the absorptions the bottom-up pass
/// deferred for open members. Returns whether anything changed.
///
/// Two classes of union state skip the pairwise absorption (collapses and dedup
/// still apply):
/// - a **cyclic unnamed** state: its rendering depends on a named subset cover
///   ([`Renderer`]), and absorbing a covered member would break the only exact
///   spelling of the cycle — conservative, spelling-invariant (the decision is a
///   function of the minimal automaton);
/// - a state with a member whose materialization **bailed**: no sound `Ty`
///   spelling exists to hand the fact oracles.
fn algebra_pass<H: Head, C: TypeContext<H>>(
    auto: &mut Automaton<'_, H>,
    root: StateId,
    ctx: &C,
) -> bool {
    let cyclic = cyclic_states(auto, root);
    let mut changed = false;
    // Member read-backs are pure in the automaton and members recur across
    // union states (a shared leaf sits in many unions), so memoize them while
    // the automaton is unmutated: any node rewrite or redirect below clears
    // the memo (a stale read-back would change absorption decisions).
    let mut read_backs: BTreeMap<StateId, Option<NormalTy<H>>> = BTreeMap::new();
    for s in auto.reachable(root) {
        let Node::Union(raw) = auto.node(s) else {
            continue;
        };
        let before: BTreeSet<StateId> = raw.iter().map(|&m| auto.resolve(m)).collect();
        let mut members: BTreeSet<StateId> = before.clone();
        members.retain(|&m| !auto.is_never(m));

        collapse_complete_enums(auto, &mut members, ctx);
        collapse_complete_bools(auto, &mut members);

        let absorb = !(cyclic.contains(&s) && auto.representative_name(s).is_none());
        // Materialize members as closed terms; canonical order = term order.
        let materialized: Option<Vec<(NormalTy<H>, StateId)>> = if absorb {
            members
                .iter()
                .map(|&m| {
                    read_backs
                        .entry(m)
                        .or_insert_with(|| read_back(auto, m))
                        .clone()
                        .map(|t| (t, m))
                })
                .collect()
        } else {
            None
        };
        let members: BTreeSet<StateId> = if let Some(mut items) = materialized {
            items.sort();

            // The pairwise absorption rule of `absorb_subtypes`, over closed terms.
            let n = items.len();
            let mut keep = vec![true; n];
            for i in 0..n {
                if items[i].0.is_sentinel() {
                    continue;
                }
                for j in 0..n {
                    if i == j || !keep[j] || items[j].0.is_sentinel() {
                        continue;
                    }
                    if !items[i]
                        .0
                        .is_subtype_of(&items[j].0, ctx, &mut HashSet::new())
                    {
                        continue;
                    }
                    let mutual = items[j]
                        .0
                        .is_subtype_of(&items[i].0, ctx, &mut HashSet::new());
                    if !mutual || j < i {
                        keep[i] = false;
                        break;
                    }
                }
            }
            items
                .into_iter()
                .zip(&keep)
                .filter(|&(_, &k)| k)
                .map(|((_, m), _)| m)
                .collect()
        } else {
            members
        };

        let state_changed = members != before;
        changed |= state_changed;
        match members.len() {
            0 => {
                auto.states[s].node = Node::Leaf(auto.never);
                read_backs.clear();
            }
            1 => {
                let m = *members
                    .first()
                    .unwrap_or_else(|| unreachable!("len checked"));
                if auto.resolve(m) != s {
                    auto.redirect_to(s, m);
                    // The redirect may point other unions' members at a union —
                    // force another closure/refinement round.
                    changed = true;
                    read_backs.clear();
                }
            }
            _ => {
                // The write normalizes the stored member Vec (resolved, sorted,
                // deduped); with the member set unchanged that is
                // read-back-invariant (read-back resolves and sorts members
                // itself), so cached read-backs stay valid.
                auto.states[s].node = Node::Union(members.into_iter().collect());
                if state_changed {
                    read_backs.clear();
                }
            }
        }
    }
    changed
}

/// Replace a complete set of an enum's variant states with the enum state
/// (`E.A | E.B | … == E`), finding or allocating it.
fn collapse_complete_enums<H: Head, C: TypeContext<H>>(
    auto: &mut Automaton<'_, H>,
    members: &mut BTreeSet<StateId>,
    ctx: &C,
) {
    let mut enums: BTreeSet<NameId> = BTreeSet::new();
    for &m in members.iter() {
        if let Node::EnumVariant(e, _) = auto.node(m) {
            enums.insert(*e);
        }
    }
    for e in enums {
        let Some(all) = ctx.enum_variants(auto.names.get(e.0)) else {
            continue; // unknown enum → no collapse (fail-safe)
        };
        let present: BTreeSet<&Name> = members
            .iter()
            .filter_map(|&m| match auto.node(m) {
                Node::EnumVariant(en, v) if *en == e => Some(auto.strs.get(v.0)),
                _ => None,
            })
            .collect();
        // ≥2 variants only — mirrors the bottom-up pass: collapsing a
        // single-variant enum would split one value set into two canonical
        // spellings (the union spelling collapses, a bare variant cannot).
        if all.len() >= 2 && all.iter().all(|v| present.contains(v)) {
            members.retain(|&m| !matches!(auto.node(m), Node::EnumVariant(en, _) if *en == e));
            let enum_state = (0..auto.states.len())
                .find(|&s| {
                    auto.redirect[s] == s && matches!(auto.node(s), Node::Enum(en) if *en == e)
                })
                .unwrap_or_else(|| auto.alloc(Node::Enum(e)));
            members.insert(enum_state);
        }
    }
}

/// Replace the complete pair of bool literal states with the `bool` state
/// (`true | false == bool` — the bool analogue of enum completeness,
/// context-free because the variant family is closed). Mirrors the bottom-up
/// pass; pre-interned leaf ids make the probes integer compares.
fn collapse_complete_bools<H: Head>(auto: &mut Automaton<'_, H>, members: &mut BTreeSet<StateId>) {
    let is_leaf = |auto: &Automaton<'_, H>, m: StateId, leaf: LeafId| matches!(auto.node(m), Node::Leaf(l) if *l == leaf);
    let has_true = members.iter().any(|&m| is_leaf(auto, m, auto.lit_true));
    let has_false = members.iter().any(|&m| is_leaf(auto, m, auto.lit_false));
    if has_true && has_false {
        members.retain(|&m| !is_leaf(auto, m, auto.lit_true) && !is_leaf(auto, m, auto.lit_false));
        let bool_state = (0..auto.states.len())
            .find(|&s| {
                auto.redirect[s] == s
                    && matches!(auto.node(s), Node::Leaf(l) if *l == auto.bool_leaf)
            })
            .unwrap_or_else(|| auto.alloc(Node::Leaf(auto.bool_leaf)));
        members.insert(bool_state);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// STAGE 4: READ-BACK & RENDERING
// ═══════════════════════════════════════════════════════════════════════════
/// An expansion of the automaton into a tree with explicit back-references —
/// the intermediate between the graph and the μ-term. `needs_binder` marks the
/// occurrences whose subtree references them (computed on the way back up, which
/// is what makes single-pass de Bruijn emission impossible directly).
enum Rb {
    Ref(StateId),
    Node {
        state: StateId,
        needs_binder: bool,
        shape: RbShape,
    },
}

enum RbShape {
    Leaf(LeafId),
    Enum(NameId),
    EnumVariant(NameId, StrId),
    Class(NameId, Vec<Rb>),
    Interface(NameId, Vec<Rb>, Vec<(StrId, Rb)>),
    List(Box<Rb>),
    Map(Box<Rb>, Box<Rb>),
    Future(Box<Rb>, Box<Rb>),
    Union(Vec<Rb>),
    Function {
        params: Vec<(Option<StrId>, FunctionParamMode, Rb)>,
        ret: Box<Rb>,
        throws: Box<Rb>,
    },
    Projection {
        base: Box<Rb>,
        interface: Box<Rb>,
        member: StrId,
    },
}

/// The canonical μ-term rooted at `root`, or `None` when the display renderer
/// bails (see [`Renderer`]). Three passes: expand the automaton to a tree with
/// back-references; convert once with placeholder displays to fix the canonical
/// union member orders; render every binder state's display; convert again with
/// the real displays. Also used by the algebra pass to materialize union
/// members as closed terms (every back-reference is wrapped on the way out, so
/// the result is always closed).
fn read_back<H: Head>(auto: &Automaton<'_, H>, root: StateId) -> Option<NormalTy<H>> {
    let cyclic = cyclic_states(auto, root);
    let (rb, _) = expand(auto, root, &mut Vec::new());

    // Pass A: canonical member orders (and the binder set). Displays are
    // placeholders — equality-transparent, so they cannot affect the sort — and
    // the pass-A term is dropped.
    let mut rec = Recorder::default();
    convert(auto, &rb, &mut Vec::new(), None, &mut rec);

    // Render every binder state's display while the automaton (which knows the
    // names, cycles, and covers) is in scope.
    let mut renderer = Renderer::new(auto, &cyclic, &rec.union_orders);
    let mut displays: BTreeMap<StateId, Ty<H>> = BTreeMap::new();
    for &s in &rec.binder_states {
        let rendered = renderer.render(s, &mut Vec::new())?;
        displays.insert(s, rendered);
    }

    // Pass B: the real term.
    let mut rec_b = Recorder::default();
    Some(convert(
        auto,
        &rb,
        &mut Vec::new(),
        Some(&displays),
        &mut rec_b,
    ))
}

/// The canonical union member orders of every union state in the expansion,
/// computed by a placeholder-display read-back — the shared prerequisite of
/// [`Renderer`] for callers that do not otherwise read back (the root render).
fn union_orders<H: Head>(
    auto: &Automaton<'_, H>,
    root: StateId,
) -> Option<BTreeMap<StateId, Vec<StateId>>> {
    let (rb, _) = expand(auto, root, &mut Vec::new());
    let mut rec = Recorder::default();
    convert(auto, &rb, &mut Vec::new(), None, &mut rec);
    Some(rec.union_orders)
}

/// Expand state `s` into a tree, cutting at on-path revisits. Returns the tree
/// and the set of states it references freely (used to decide `needs_binder`
/// bottom-up).
fn expand<H: Head>(
    auto: &Automaton<'_, H>,
    s: StateId,
    path: &mut Vec<StateId>,
) -> (Rb, BTreeSet<StateId>) {
    let r = auto.resolve(s);
    if path.contains(&r) {
        return (Rb::Ref(r), BTreeSet::from([r]));
    }
    path.push(r);
    let mut refs = BTreeSet::new();
    let child = |auto: &Automaton<'_, H>,
                 c: StateId,
                 path: &mut Vec<StateId>,
                 refs: &mut BTreeSet<StateId>| {
        let (rb, r) = expand(auto, c, path);
        refs.extend(r);
        rb
    };
    let shape = match auto.node(r) {
        Node::Leaf(l) => RbShape::Leaf(*l),
        Node::Enum(qn) => RbShape::Enum(*qn),
        Node::EnumVariant(qn, v) => RbShape::EnumVariant(*qn, *v),
        Node::Class(qn, args) => {
            let (qn, args) = (*qn, args.clone());
            RbShape::Class(
                qn,
                args.into_iter()
                    .map(|a| child(auto, a, path, &mut refs))
                    .collect(),
            )
        }
        Node::Interface(qn, args, bindings) => {
            let (qn, args, bindings) = (*qn, args.clone(), bindings.clone());
            RbShape::Interface(
                qn,
                args.into_iter()
                    .map(|a| child(auto, a, path, &mut refs))
                    .collect(),
                bindings
                    .into_iter()
                    .map(|(n, t)| (n, child(auto, t, path, &mut refs)))
                    .collect(),
            )
        }
        Node::List(inner) => {
            let inner = *inner;
            RbShape::List(Box::new(child(auto, inner, path, &mut refs)))
        }
        Node::Map(k, v) => {
            let (k, v) = (*k, *v);
            RbShape::Map(
                Box::new(child(auto, k, path, &mut refs)),
                Box::new(child(auto, v, path, &mut refs)),
            )
        }
        Node::Future(v, e) => {
            let (v, e) = (*v, *e);
            RbShape::Future(
                Box::new(child(auto, v, path, &mut refs)),
                Box::new(child(auto, e, path, &mut refs)),
            )
        }
        Node::Union(members) => RbShape::Union(
            members
                .clone()
                .into_iter()
                .map(|m| child(auto, m, path, &mut refs))
                .collect(),
        ),
        Node::Function {
            params,
            ret,
            throws,
        } => {
            let (params, ret, throws) = (params.clone(), *ret, *throws);
            RbShape::Function {
                params: params
                    .into_iter()
                    .map(|(n, m, t)| (n, m, child(auto, t, path, &mut refs)))
                    .collect(),
                ret: Box::new(child(auto, ret, path, &mut refs)),
                throws: Box::new(child(auto, throws, path, &mut refs)),
            }
        }
        Node::Projection {
            base,
            interface,
            member,
        } => {
            let (base, interface, member) = (*base, *interface, *member);
            RbShape::Projection {
                base: Box::new(child(auto, base, path, &mut refs)),
                interface: Box::new(child(auto, interface, path, &mut refs)),
                member,
            }
        }
    };
    path.pop();
    let needs_binder = refs.remove(&r);
    (
        Rb::Node {
            state: r,
            needs_binder,
            shape,
        },
        refs,
    )
}

/// What a convert pass records for the passes after it: the canonical member
/// order of every union state (fixed by pass A's term sort, consumed by the
/// renderer), and the set of states that emit binders (whose displays pass B
/// needs).
#[derive(Default)]
struct Recorder {
    union_orders: BTreeMap<StateId, Vec<StateId>>,
    binder_states: BTreeSet<StateId>,
}

/// Convert the expansion to the canonical μ-term: binder-emitting ancestors form
/// the de Bruijn context, union members sort by the canonical `Ord`, and each
/// emitted binder gets its [`MuDisplay`]. With `displays: None` (pass A) the
/// display payload is a placeholder — [`MuDisplay`] is equality-transparent, so
/// placeholders cannot affect the member sort — and only the [`Recorder`]
/// output matters. Owned values are cloned out of the interners here — the
/// single clone point of the pipeline.
fn convert<H: Head>(
    auto: &Automaton<'_, H>,
    rb: &Rb,
    binders: &mut Vec<StateId>,
    displays: Option<&BTreeMap<StateId, Ty<H>>>,
    rec: &mut Recorder,
) -> NormalTy<H> {
    match rb {
        Rb::Ref(s) => {
            let index = binders
                .iter()
                .rev()
                .position(|&b| b == *s)
                .unwrap_or_else(|| {
                    unreachable!(
                        "a back-reference targets an ancestor, and every referenced \
                         ancestor emits a binder"
                    )
                });
            NormalTy::RecVar(index as u32)
        }
        Rb::Node {
            state,
            needs_binder,
            shape,
        } => {
            if *needs_binder {
                binders.push(*state);
                rec.binder_states.insert(*state);
            }
            let body = match shape {
                RbShape::Leaf(l) => auto.leaves.get(l.0).clone(),
                RbShape::Enum(qn) => NormalTy::Enum(auto.names.get(qn.0).clone()),
                RbShape::EnumVariant(qn, v) => {
                    NormalTy::EnumVariant(auto.names.get(qn.0).clone(), auto.strs.get(v.0).clone())
                }
                RbShape::Class(qn, args) => NormalTy::Class(
                    auto.names.get(qn.0).clone(),
                    args.iter()
                        .map(|a| convert(auto, a, binders, displays, rec))
                        .collect(),
                ),
                RbShape::Interface(qn, args, bindings) => NormalTy::Interface(
                    auto.names.get(qn.0).clone(),
                    args.iter()
                        .map(|a| convert(auto, a, binders, displays, rec))
                        .collect(),
                    bindings
                        .iter()
                        .map(|(n, t)| {
                            (
                                auto.strs.get(n.0).clone(),
                                convert(auto, t, binders, displays, rec),
                            )
                        })
                        .collect(),
                ),
                RbShape::List(inner) => {
                    NormalTy::List(Box::new(convert(auto, inner, binders, displays, rec)))
                }
                RbShape::Map(k, v) => NormalTy::Map {
                    key: Box::new(convert(auto, k, binders, displays, rec)),
                    value: Box::new(convert(auto, v, binders, displays, rec)),
                },
                RbShape::Future(v, e) => NormalTy::Future(
                    Box::new(convert(auto, v, binders, displays, rec)),
                    Box::new(convert(auto, e, binders, displays, rec)),
                ),
                RbShape::Union(members) => {
                    let mut converted: Vec<(NormalTy<H>, StateId)> = members
                        .iter()
                        .map(|m| {
                            let state = match m {
                                Rb::Ref(s) => *s,
                                Rb::Node { state, .. } => *state,
                            };
                            (convert(auto, m, binders, displays, rec), state)
                        })
                        .collect();
                    converted.sort();
                    converted.dedup_by(|a, b| a.0 == b.0);
                    rec.union_orders
                        .entry(*state)
                        .or_insert_with(|| converted.iter().map(|(_, s)| *s).collect());
                    let mut members: Vec<NormalTy<H>> =
                        converted.into_iter().map(|(t, _)| t).collect();
                    match members.len() {
                        0 => NormalTy::Never,
                        1 => members.pop().unwrap_or_else(|| unreachable!("len checked")),
                        _ => NormalTy::Union(members),
                    }
                }
                RbShape::Function {
                    params,
                    ret,
                    throws,
                } => NormalTy::Function {
                    params: params
                        .iter()
                        .map(|(name, mode, t)| NormalParam {
                            name: name.map(|n| auto.strs.get(n.0).clone()),
                            ty: convert(auto, t, binders, displays, rec),
                            mode: *mode,
                        })
                        .collect(),
                    ret: Box::new(convert(auto, ret, binders, displays, rec)),
                    throws: Box::new(convert(auto, throws, binders, displays, rec)),
                },
                RbShape::Projection {
                    base,
                    interface,
                    member,
                } => NormalTy::AssociatedTypeProjection {
                    base: Box::new(convert(auto, base, binders, displays, rec)),
                    interface: Box::new(convert(auto, interface, binders, displays, rec)),
                    member: auto.strs.get(member.0).clone(),
                },
            };
            if *needs_binder {
                binders.pop();
                let rendered = match displays {
                    Some(map) => map
                        .get(state)
                        .cloned()
                        .unwrap_or_else(|| unreachable!("pass A recorded every binder state")),
                    // Pass A placeholder: never escapes (the pass-A term is
                    // dropped), and equality-transparency keeps it out of the
                    // member sort. `Error` is the honest sentinel for "not a
                    // real rendering".
                    None => Ty::Error {
                        attr: TyAttr::default(),
                    },
                };
                NormalTy::Mu {
                    binder: MuDisplay {
                        name: auto.representative_name(*state).cloned(),
                        rendered: Box::new(rendered),
                    },
                    body: Box::new(body),
                }
            } else {
                body
            }
        }
    }
}

/// States on a cycle (in a nontrivial SCC of the full edge graph, or carrying a
/// self-edge) — the states whose alias names cut recursion in the rendering.
fn cyclic_states<H: Head>(auto: &Automaton<'_, H>, root: StateId) -> BTreeSet<StateId> {
    struct Tarjan<'x, 'a, H: Head> {
        auto: &'x Automaton<'a, H>,
        index: BTreeMap<StateId, usize>,
        low: BTreeMap<StateId, usize>,
        on_stack: BTreeSet<StateId>,
        stack: Vec<StateId>,
        next: usize,
        cyclic: BTreeSet<StateId>,
    }
    impl<H: Head> Tarjan<'_, '_, H> {
        fn visit(&mut self, s: StateId) {
            self.index.insert(s, self.next);
            self.low.insert(s, self.next);
            self.next += 1;
            self.stack.push(s);
            self.on_stack.insert(s);
            let succs: Vec<StateId> = children(self.auto.node(s))
                .into_iter()
                .map(|c| self.auto.resolve(c))
                .collect();
            let mut self_edge = false;
            for t in succs {
                if t == s {
                    self_edge = true;
                    continue;
                }
                if !self.index.contains_key(&t) {
                    self.visit(t);
                    let l = self.low[&t].min(self.low[&s]);
                    self.low.insert(s, l);
                } else if self.on_stack.contains(&t) {
                    let l = self.index[&t].min(self.low[&s]);
                    self.low.insert(s, l);
                }
            }
            if self.low[&s] == self.index[&s] {
                let mut scc = Vec::new();
                loop {
                    let t = self
                        .stack
                        .pop()
                        .unwrap_or_else(|| unreachable!("Tarjan stack underflow"));
                    self.on_stack.remove(&t);
                    scc.push(t);
                    if t == s {
                        break;
                    }
                }
                if scc.len() > 1 {
                    self.cyclic.extend(scc);
                } else if self_edge {
                    self.cyclic.insert(s);
                }
            }
        }
    }
    let mut t = Tarjan {
        auto,
        index: BTreeMap::new(),
        low: BTreeMap::new(),
        on_stack: BTreeSet::new(),
        stack: Vec::new(),
        next: 0,
        cyclic: BTreeSet::new(),
    };
    for s in auto.reachable(root) {
        if !t.index.contains_key(&s) {
            t.visit(s);
        }
    }
    t.cyclic
}

/// The named-cut display renderer: recursion folds to an alias name at every
/// *named cyclic* state, and — when ε-closure has spliced the named union
/// through, leaving its cycle nameless — to a **subset cover**: an unnamed
/// cyclic union `v` whose members include everything a named cyclic union `u`
/// holds renders as `u | (members(v) − members(u))`, which is exact (union
/// semantics) and cuts every cycle through `v`. Renders are memoized per state
/// (completed renders are path-independent: every cycle cut is by name or
/// cover, never by position).
///
/// **Bail (`None`)** when an on-path state re-enters with no exact cut — an
/// exotic cycle (e.g. covers broken by absorption interleavings) whose tree has
/// no finite alias-based `Ty` spelling. A wrong display must never reach a fact
/// oracle, so the caller degrades to the pre-automaton form instead
/// (sound, less canonical).
struct Renderer<'x, 'a, H: Head> {
    auto: &'x Automaton<'a, H>,
    cyclic: &'x BTreeSet<StateId>,
    orders: &'x BTreeMap<StateId, Vec<StateId>>,
    /// Named union states — the cover candidates — with their resolved member
    /// sets, in representative-name order (deterministic cover choice). NOT
    /// restricted to cyclic states: the splice-bypass case leaves the named
    /// union *off* the surviving cycle, and its name denotes its tree either
    /// way.
    candidates: Vec<(StateId, BTreeSet<StateId>)>,
    memo: BTreeMap<StateId, Ty<H>>,
}

impl<'x, 'a, H: Head> Renderer<'x, 'a, H> {
    fn new(
        auto: &'x Automaton<'a, H>,
        cyclic: &'x BTreeSet<StateId>,
        orders: &'x BTreeMap<StateId, Vec<StateId>>,
    ) -> Self {
        // The whole automaton, not the reachable cone: an ε-splice bypasses the
        // named binder union, leaving it *unreachable* from the root — and that
        // orphaned state is exactly the cover the surviving cycle needs.
        let mut candidates: Vec<(StateId, BTreeSet<StateId>)> = (0..auto.states.len())
            .filter(|&s| {
                auto.redirect[s] == s
                    && auto.representative_name(s).is_some()
                    && matches!(auto.node(s), Node::Union(_))
            })
            .map(|s| {
                let Node::Union(members) = auto.node(s) else {
                    unreachable!("filtered to unions")
                };
                (s, members.iter().map(|&m| auto.resolve(m)).collect())
            })
            .collect();
        candidates.sort_by(|(a, _), (b, _)| {
            auto.representative_name(*a)
                .cmp(&auto.representative_name(*b))
        });
        Self {
            auto,
            cyclic,
            orders,
            candidates,
            memo: BTreeMap::new(),
        }
    }

    fn render(&mut self, s: StateId, path: &mut Vec<StateId>) -> Option<Ty<H>> {
        let r = self.auto.resolve(s);
        if let Some(t) = self.memo.get(&r) {
            return Some(t.clone());
        }
        if self.cyclic.contains(&r)
            && let Some(name) = self.auto.representative_name(r)
        {
            let t = Ty::TypeAlias(name.clone(), TyAttr::default());
            self.memo.insert(r, t.clone());
            return Some(t);
        }
        if path.contains(&r) {
            // On-path re-entry with no name: only a union fully covered by
            // named states can still cut the cycle exactly; anything else has
            // no finite alias-based spelling — bail.
            return self.cover_only(r);
        }
        path.push(r);
        let out = self.structural(r, path);
        path.pop();
        if let Some(t) = &out {
            self.memo.insert(r, t.clone());
        }
        out
    }

    /// The named covers of union state `r`: candidates whose member sets are
    /// subsets of `r`'s, plus the members they leave uncovered.
    fn covers_of(&self, r: StateId) -> (Vec<StateId>, BTreeSet<StateId>) {
        let Node::Union(members) = self.auto.node(r) else {
            return (Vec::new(), BTreeSet::new());
        };
        let members: BTreeSet<StateId> = members.iter().map(|&m| self.auto.resolve(m)).collect();
        let mut covers = Vec::new();
        let mut covered: BTreeSet<StateId> = BTreeSet::new();
        for (u, u_members) in &self.candidates {
            if *u != r && u_members.is_subset(&members) {
                covers.push(*u);
                covered.extend(u_members.iter().copied());
            }
        }
        let uncovered = members.difference(&covered).copied().collect();
        (covers, uncovered)
    }

    /// Render an on-path unnamed state via covers alone: exact only when the
    /// covers leave nothing uncovered.
    fn cover_only(&mut self, r: StateId) -> Option<Ty<H>> {
        if !matches!(self.auto.node(r), Node::Union(_)) {
            return None;
        }
        let (covers, uncovered) = self.covers_of(r);
        if covers.is_empty() || !uncovered.is_empty() {
            return None;
        }
        self.cover_union(&covers, &[], &mut Vec::new())
    }

    /// Assemble a union `Ty` from cover names plus rendered extras.
    fn cover_union(
        &mut self,
        covers: &[StateId],
        extras: &[StateId],
        path: &mut Vec<StateId>,
    ) -> Option<Ty<H>> {
        let attr = TyAttr::default;
        let mut parts: Vec<Ty<H>> = Vec::new();
        for &u in covers {
            let name = self
                .auto
                .representative_name(u)
                .unwrap_or_else(|| unreachable!("candidates are named"));
            parts.push(Ty::TypeAlias(name.clone(), attr()));
        }
        for &m in extras {
            parts.push(self.render(m, path)?);
        }
        Some(match parts.len() {
            0 => Ty::Never { attr: attr() },
            1 => parts.pop().unwrap_or_else(|| unreachable!("len checked")),
            _ => Ty::Union(parts, attr()),
        })
    }

    /// Render `r`'s node one level, children through [`Self::render`]. Public
    /// to the module so the root render can unfold a *named* root once
    /// (exposing its head constructor) while nested occurrences fold to the
    /// name.
    fn structural(&mut self, r: StateId, path: &mut Vec<StateId>) -> Option<Ty<H>> {
        let attr = TyAttr::default;
        let auto = self.auto;
        Some(match auto.node(r) {
            Node::Leaf(l) => auto.leaves.get(l.0).clone().into_ty(),
            Node::Enum(qn) => Ty::Enum(auto.names.get(qn.0).clone(), attr()),
            Node::EnumVariant(qn, v) => Ty::EnumVariant(
                auto.names.get(qn.0).clone(),
                auto.strs.get(v.0).clone(),
                attr(),
            ),
            Node::Class(qn, args) => {
                let (qn, args) = (*qn, args.clone());
                let mut out = Vec::with_capacity(args.len());
                for a in args {
                    out.push(self.render(a, path)?);
                }
                Ty::Class(auto.names.get(qn.0).clone(), out, attr())
            }
            Node::Interface(qn, args, bindings) => {
                let (qn, args, bindings) = (*qn, args.clone(), bindings.clone());
                let mut out_args = Vec::with_capacity(args.len());
                for a in args {
                    out_args.push(self.render(a, path)?);
                }
                let mut out_bindings = Vec::with_capacity(bindings.len());
                for (n, t) in bindings {
                    out_bindings.push((auto.strs.get(n.0).clone(), self.render(t, path)?));
                }
                Ty::Interface(auto.names.get(qn.0).clone(), out_args, out_bindings, attr())
            }
            Node::List(inner) => {
                let inner = *inner;
                Ty::List(Box::new(self.render(inner, path)?), attr())
            }
            Node::Map(k, v) => {
                let (k, v) = (*k, *v);
                Ty::Map {
                    key: Box::new(self.render(k, path)?),
                    value: Box::new(self.render(v, path)?),
                    attr: attr(),
                }
            }
            Node::Future(v, e) => {
                let (v, e) = (*v, *e);
                Ty::Future(
                    Box::new(self.render(v, path)?),
                    Box::new(self.render(e, path)?),
                    attr(),
                )
            }
            Node::Union(_) => {
                // Cyclic unnamed unions fold their covered members to alias
                // names — the splice-bypass case (`type A = int | (A | null)[]`
                // renders the inner union as `A | null`). Everything else
                // renders member-wise in the canonical order fixed by pass A.
                let ordered: Vec<StateId> = match self.orders.get(&r) {
                    Some(o) => o.clone(),
                    // Defensive: a union outside the recorded expansion —
                    // resolved, deduplicated, id-ordered (deterministic).
                    None => {
                        let Node::Union(members) = auto.node(r) else {
                            unreachable!("matched Union above")
                        };
                        let set: BTreeSet<StateId> =
                            members.iter().map(|&m| auto.resolve(m)).collect();
                        set.into_iter().collect()
                    }
                };
                if self.cyclic.contains(&r) && auto.representative_name(r).is_none() {
                    let (covers, _) = self.covers_of(r);
                    let covered: BTreeSet<StateId> = self
                        .candidates
                        .iter()
                        .filter(|(u, _)| covers.contains(u))
                        .flat_map(|(_, ms)| ms.iter().copied())
                        .collect();
                    let extras: Vec<StateId> = ordered
                        .iter()
                        .copied()
                        .filter(|m| !covered.contains(m))
                        .collect();
                    if covers.is_empty() {
                        // No name and no cover: the members render structurally
                        // only if no cycle re-enters this state (the `render`
                        // on-path check catches re-entry and bails).
                        let mut parts = Vec::with_capacity(ordered.len());
                        for m in ordered {
                            parts.push(self.render(m, path)?);
                        }
                        return Some(match parts.len() {
                            0 => Ty::Never { attr: attr() },
                            1 => parts.pop().unwrap_or_else(|| unreachable!("len checked")),
                            _ => Ty::Union(parts, attr()),
                        });
                    }
                    return self.cover_union(&covers, &extras, path);
                }
                let mut parts = Vec::with_capacity(ordered.len());
                for m in ordered {
                    parts.push(self.render(m, path)?);
                }
                match parts.len() {
                    0 => Ty::Never { attr: attr() },
                    1 => parts.pop().unwrap_or_else(|| unreachable!("len checked")),
                    _ => Ty::Union(parts, attr()),
                }
            }
            Node::Function {
                params,
                ret,
                throws,
            } => {
                let (params, ret, throws) = (params.clone(), *ret, *throws);
                let mut out_params = Vec::with_capacity(params.len());
                for (name, mode, t) in params {
                    out_params.push(crate::FunctionParamTy {
                        name: name.map(|n| auto.strs.get(n.0).clone()),
                        ty: self.render(t, path)?,
                        mode,
                    });
                }
                Ty::Function {
                    params: out_params,
                    ret: Box::new(self.render(ret, path)?),
                    throws: Box::new(self.render(throws, path)?),
                    attr: attr(),
                }
            }
            Node::Projection {
                base,
                interface,
                member,
            } => {
                let (base, interface, member) = (*base, *interface, *member);
                let iface = match auto.node(interface) {
                    Node::Interface(name, args, bindings) => {
                        let (name, args, bindings) = (*name, args.clone(), bindings.clone());
                        let mut out_args = Vec::with_capacity(args.len());
                        for a in args {
                            out_args.push(self.render(a, path)?);
                        }
                        let mut out_bindings = Vec::with_capacity(bindings.len());
                        for (n, t) in bindings {
                            out_bindings.push((auto.strs.get(n.0).clone(), self.render(t, path)?));
                        }
                        crate::Interface {
                            name: auto.names.get(name.0).clone(),
                            generics: out_args,
                            associated_types: out_bindings,
                        }
                    }
                    _ => unreachable!("projection qualifier is an interface"),
                };
                Ty::AssociatedTypeProjection {
                    base: Box::new(self.render(base, path)?),
                    interface: Box::new(iface),
                    member: auto.strs.get(member.0).clone(),
                    attr: attr(),
                }
            }
        })
    }
}
