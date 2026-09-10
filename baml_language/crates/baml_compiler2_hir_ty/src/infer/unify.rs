//! The inference table: an ena union-find over [`InferVar`]s with
//! snapshot/rollback and eager, occurs-checked `Eq` unification - the
//! rust-analyzer `InferenceTable` shape, per the constraint-system design in
//! this crate's README.
//!
//! S5 scope: equality only. The settled `VarData` bounds
//! (lowers/uppers/obligations for `Sub` constraints and the obligation
//! worklist) join with the first `Sub` constraints; until then a variable's
//! class carries its solver state and its policy (`VarValue`). Policy
//! lives INSIDE the ena value - the undo log must govern it, or a rollback
//! frees an index whose stale policy then misclassifies the variable that
//! reuses it.
//!
//! Unification discipline (rustc's `TypeVariableValue` model): both sides are
//! shallow-resolved before relating, so two `Known` roots never merge inside
//! ena's pure value-merge - a known root is unified structurally against the
//! other side instead. `Error` unifies with everything (a diagnostic was
//! already emitted; never cascade). Unions unify positionally for now: the
//! ACI-equality cases (reordered/var-bearing unions in invariant positions)
//! are the deferred-with-budget class that arrives with `Sub` constraints.

use baml_compiler2_ast::ExprId;
use baml_type::{
    ParamTy,
    interned::{InferTy, InferVar, Ty, for_each_child},
};
use ena::unify as ut;
use rustc_hash::{FxHashMap, FxHashSet};

/// Local ena key for [`InferVar`] (orphan rules keep `UnifyKey` out of
/// `baml_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct VarKey(InferVar);

impl ut::UnifyKey for VarKey {
    type Value = VarValue;

    fn index(&self) -> u32 {
        self.0.index()
    }

    fn from_index(index: u32) -> VarKey {
        VarKey(InferVar::new(index))
    }

    fn tag() -> &'static str {
        "VarKey"
    }
}

/// What a variable IS - one total axis, from which every behavior derives
/// (the predicates below). Carried inside the ena value - never in a side
/// table keyed by creation index - so the undo log governs it: a rollback
/// that frees an index for reuse also reverts its policy. (The side-table
/// version survived rollback, so a fresh VALUE variable reusing a freed
/// index inherited the old kind and could silently default to `never` as
/// an "effect".)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VarPolicy {
    /// An ordinary value variable (call instantiations, holes): demands
    /// must agree, and an unconstrained class is an error (ruling 2).
    #[default]
    Value,
    /// A throws-channel variable: unconstrained defaults to `never` at
    /// finalize - BAML's only defaulting rule (S12).
    Effect,
    /// An unannotated lambda parameter: monomorphic source, initially
    /// untyped, so its first ground demand commits and later incompatible
    /// demands diagnose at their own use sites.
    LambdaParam,
    /// Element/key/value of an EMPTY container literal (the honest
    /// replacement for TIR's Evolving sentinels): first-demand order on
    /// disagreeing demands (ruling 1), and a ground `unknown` demand
    /// commits the slot to top - TIR's frozen-Evolving behavior at
    /// exactly the demanded case.
    ContainerSlot,
}

impl VarPolicy {
    /// Whether the first ground demand commits the class, with later
    /// incompatible demands reporting at their own sites - where an
    /// ordinary var (a call instantiation) fails resolution instead.
    pub fn first_demand_commits(self) -> bool {
        match self {
            VarPolicy::LambdaParam | VarPolicy::ContainerSlot => true,
            VarPolicy::Value | VarPolicy::Effect => false,
        }
    }

    /// Whether a ground `unknown` demand commits the class to the top type.
    /// ONLY container-shaped slots absorb: committing an ordinary or
    /// lambda-parameter variable to `unknown` would poison its real
    /// solution (and launder "couldn't infer" into `unknown`).
    pub fn absorbs_unknown(self) -> bool {
        match self {
            VarPolicy::ContainerSlot => true,
            VarPolicy::Value | VarPolicy::Effect | VarPolicy::LambdaParam => false,
        }
    }

    /// Whether an unconstrained class defaults to `never` at finalize
    /// (S12) instead of erroring.
    pub fn defaults_to_never(self) -> bool {
        match self {
            VarPolicy::Effect => true,
            VarPolicy::Value | VarPolicy::LambdaParam | VarPolicy::ContainerSlot => false,
        }
    }

    /// The class policy after a var-var union. `Value` is the identity; a
    /// lambda parameter absorbed into a container class takes that class's
    /// policy (its behavior set is a strict superset - real case:
    /// `let xs = []; xs.push(x)` inside a lambda unions the element slot with
    /// the parameter). An effect class joining any specialized class is not
    /// constructible
    /// under the current minting discipline (effects only ever unify with
    /// throws slots, which are ground, effect vars, or plain hole vars);
    /// debug-assert and keep the effect policy - a throws channel cannot
    /// afford to lose its `never` default.
    fn join(self, other: VarPolicy) -> VarPolicy {
        match (self, other) {
            (VarPolicy::Effect, VarPolicy::Effect) => VarPolicy::Effect,
            (VarPolicy::Effect, mixed) | (mixed, VarPolicy::Effect) => {
                debug_assert!(
                    matches!(mixed, VarPolicy::Value),
                    "effect class unified with a {mixed:?} class"
                );
                VarPolicy::Effect
            }
            (VarPolicy::Value, other) | (other, VarPolicy::Value) => other,
            (VarPolicy::LambdaParam, other) | (other, VarPolicy::LambdaParam) => other,
            (VarPolicy::ContainerSlot, VarPolicy::ContainerSlot) => VarPolicy::ContainerSlot,
        }
    }
}

/// Solver state of a variable's equivalence class.
///
/// Policy is UNSOLVED-ONLY state, so it lives inside that variant: every
/// behavior it drives (first-demand order, `unknown` absorption, the
/// `never` default) is consulted only while the class is open, and a
/// var-var union merges only open classes (`unify` shallow-resolves both
/// sides first). Solving retires the policy - a solved class IS its
/// solution, nothing more. A rollback of the solving step restores the
/// `Unsolved` value, policy included, through the ena undo log.
#[derive(Debug, Clone, PartialEq)]
enum VarValue {
    Unsolved {
        policy: VarPolicy,
        /// The block-scope depth the class was minted in (see
        /// [`InferenceTable::bind_scoped_param`]): a solution may mention
        /// a scoped `type T = …` parameter only from a depth at most this.
        /// A var-var union keeps the shallower depth, since the merged
        /// class is observable from the outer scope.
        universe: u32,
    },
    Solved(Ty),
}

impl ut::UnifyValue for VarValue {
    type Error = ut::NoError;

    fn unify_values(a: &VarValue, b: &VarValue) -> Result<VarValue, ut::NoError> {
        Ok(match (a, b) {
            (VarValue::Solved(_), VarValue::Solved(_)) => unreachable!(
                "unify shallow-resolves before relating, so two solved roots never merge"
            ),
            // The solving moment (`bind`/`solve` union a solution into an
            // open class): the policy has done its job and retires.
            (VarValue::Solved(ty), VarValue::Unsolved { .. })
            | (VarValue::Unsolved { .. }, VarValue::Solved(ty)) => VarValue::Solved(ty.clone()),
            (
                VarValue::Unsolved {
                    policy: a,
                    universe: ua,
                },
                VarValue::Unsolved {
                    policy: b,
                    universe: ub,
                },
            ) => VarValue::Unsolved {
                policy: a.join(*b),
                universe: (*ua).min(*ub),
            },
        })
    }
}

/// Every inference variable `ty` names, in walk order (`ty` is expected to
/// be resolved, so each is an open class or an alias of one).
fn collect_infer_vars(ty: &Ty, out: &mut Vec<InferVar>) {
    if !ty.has_infer() {
        return;
    }
    if let InferTy::InferVar { var, .. } = ty.kind() {
        out.push(*var);
        return;
    }
    for_each_child(ty.kind(), |child| collect_infer_vars(child, out));
}

/// A structural mismatch: the innermost pair of types that failed to unify.
/// `escaped` names the block-scoped parameter when the failure is a
/// solution that would leave its block (`right` mentions it; `left` is the
/// variable), rather than a shape mismatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnifyError {
    pub left: Ty,
    pub right: Ty,
    pub escaped: Option<ParamTy>,
}

/// One deposited bound and where it was deposited: the expression the
/// relation was checked at, when the road had one (a pattern walk has
/// none). A bound that turns out to carry a block-scoped parameter past
/// its block is reported at its anchor.
#[derive(Debug, Clone, PartialEq)]
pub struct Bound {
    pub ty: Ty,
    pub anchor: Option<ExprId>,
}

/// Sub-constraint evidence accumulated on an unsolved variable's class:
/// lower bounds are values flowing INTO the variable, upper bounds are
/// contexts it flows into. Resolution derives the solution from these
/// (widen fresh lowers, lowers must agree, checked against the uppers).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VarBounds {
    pub lowers: Vec<Bound>,
    pub uppers: Vec<Bound>,
}

/// A revertible point in the table's history; see
/// [`InferenceTable::snapshot`]. The bounds map is snapshotted by clone
/// (rust-analyzer snapshots its fulfillment context the same way); the ena
/// undo log covers the union-find AND every class's [`VarPolicy`] - the whole
/// of the table's remaining state, so nothing survives a rollback.
pub struct Snapshot {
    vars: ut::Snapshot<ut::InPlace<VarKey>>,
    bounds: FxHashMap<u32, VarBounds>,
    /// The scope depth the snapshot was taken at. Not restored - a probe is
    /// expected to open and close scopes symmetrically, and the rollback
    /// asserts it did.
    universe: u32,
}

#[derive(Default)]
pub struct InferenceTable {
    vars: ut::InPlaceUnificationTable<VarKey>,
    /// Bounds per CLASS, keyed by the root's index; var-var unions merge the
    /// two roots' entries.
    bounds: FxHashMap<u32, VarBounds>,
    /// The current block-scope depth: how many `type T = …` bindings are
    /// open. Fresh variables record it; see
    /// [`InferenceTable::bind_scoped_param`]. Not part of a snapshot: the
    /// walk opens and closes scopes symmetrically inside any probe.
    universe: u32,
    /// The depth each scoped parameter was bound at, by parameter index.
    /// Append-only (a statement binds its parameter once), so it needs no
    /// rollback either.
    scoped_params: FxHashMap<u32, u32>,
}

impl InferenceTable {
    pub fn new() -> InferenceTable {
        InferenceTable::default()
    }

    /// Allocates a fresh, unconstrained VALUE variable.
    pub fn new_var(&mut self) -> InferVar {
        self.new_var_of(VarPolicy::Value)
    }

    fn new_var_of(&mut self, policy: VarPolicy) -> InferVar {
        self.vars
            .new_key(VarValue::Unsolved {
                policy,
                universe: self.universe,
            })
            .0
    }

    /// Opens the scope of a block-level `type T = …` binding: variables
    /// minted from here until the matching
    /// [`InferenceTable::close_scopes_to`] live one universe deeper and may
    /// be solved to `param`; variables minted outside may not (rustc's
    /// universe check on placeholders - an outer `let xs = []` must not
    /// become `T[]` through a push inside the block). The scope opens at
    /// the STATEMENT, not the block: a value created earlier in the same
    /// block has its type fixed at allocation, and the parameter's frame
    /// slot is bound only when the statement runs, so that value cannot
    /// carry `T` even though it dies with the block.
    ///
    /// Depths are reused: two sibling blocks each binding one parameter both
    /// open universe `n + 1`. That is sound because a universe only ever
    /// compares against the depth a parameter is registered at, and closing a
    /// scope retires its parameters to a depth no variable can reach (see
    /// [`InferenceTable::close_scopes_to`]) - so a sibling that reuses the
    /// depth cannot take the earlier block's parameter even though the two
    /// blocks opened the same number.
    pub fn bind_scoped_param(&mut self, param: &ParamTy) {
        self.universe += 1;
        let previous = self.scoped_params.insert(param.index(), self.universe);
        debug_assert!(
            previous.is_none(),
            "a statement binds its parameter once, so a depth is never overwritten"
        );
    }

    /// Closes every scope deeper than `depth`, the enclosing block's count of
    /// open bindings.
    ///
    /// A closed parameter is RETIRED rather than left at its old depth: it
    /// keeps a registration, so a later mention still counts as escaping, but
    /// at a depth deeper than any variable can be minted at, so the ordinary
    /// check refuses it for every variable - including one in a sibling block
    /// that reuses the depth this one just gave up.
    pub fn close_scopes_to(&mut self, depth: u32) {
        debug_assert!(depth <= self.universe, "scopes close innermost first");
        for registered in self.scoped_params.values_mut() {
            if *registered > depth {
                *registered = u32::MAX;
            }
        }
        self.universe = depth;
    }

    /// The root of every still-open class minted deeper than `depth`: the
    /// variables a closing block leaves behind, in index order.
    pub fn unsolved_vars_deeper_than(&mut self, depth: u32) -> Vec<InferVar> {
        let len = u32::try_from(self.vars.len())
            .unwrap_or_else(|_| unreachable!("variable count fits in u32"));
        let mut out = Vec::new();
        for index in 0..len {
            let key = VarKey(InferVar::new(index));
            if self.vars.find(key).0.index() != index {
                continue;
            }
            if let VarValue::Unsolved { universe, .. } = self.vars.probe_value(key)
                && universe > depth
            {
                out.push(InferVar::new(index));
            }
        }
        out
    }

    /// Moves `var`'s open class out to `depth`: the block that minted it
    /// has closed, so the class is observable from the enclosing scope and
    /// may no longer take that block's parameters as a solution. A class
    /// already at or above `depth`, or solved, is left alone.
    pub fn demote_to(&mut self, var: InferVar, depth: u32) {
        let VarValue::Unsolved { policy, universe } = self.vars.probe_value(VarKey(var)) else {
            return;
        };
        if universe <= depth {
            return;
        }
        self.vars.union_value(
            VarKey(var),
            VarValue::Unsolved {
                policy,
                universe: depth,
            },
        );
    }

    /// The first block-scoped parameter in `ty` (through solved variables)
    /// that `var`'s class may not take as a solution: one bound deeper than
    /// the class was minted, or one this table never saw bound. `None` for
    /// a solved class - its solution was judged when it was made.
    pub fn escaping_scoped_param(&mut self, var: InferVar, ty: &Ty) -> Option<ParamTy> {
        fn walk(
            scoped_params: &FxHashMap<u32, u32>,
            universe: u32,
            ty: &Ty,
            escaped: &mut Option<ParamTy>,
        ) {
            if escaped.is_some() || !ty.has_typevar() {
                return;
            }
            if let InferTy::TypeVar(param, _) = ty.kind()
                && param.is_scoped()
                && scoped_params
                    .get(&param.index())
                    .is_none_or(|depth| *depth > universe)
            {
                *escaped = Some(param.clone());
                return;
            }
            for_each_child(ty.kind(), |child| {
                walk(scoped_params, universe, child, escaped);
            });
        }
        // No body binds a scoped parameter in the common case, and this
        // sits on every solve and bound deposit: skip the resolution walk.
        if self.scoped_params.is_empty() {
            return None;
        }
        let VarValue::Unsolved { universe, .. } = self.vars.probe_value(VarKey(var)) else {
            return None;
        };
        let ty = self.resolve_completely(ty);
        let mut escaped = None;
        walk(&self.scoped_params, universe, &ty, &mut escaped);
        escaped
    }

    /// Allocates a fresh variable of `policy`, as a type.
    pub fn new_var_ty_of(&mut self, policy: VarPolicy) -> Ty {
        Ty::infer_var(self.new_var_of(policy))
    }

    /// What `var`'s still-open equivalence class IS; behaviors derive
    /// from it ([`VarPolicy`]'s predicates). `None` once solved - policy
    /// retires at solution, so a solved class has none to ask about.
    pub fn unsolved_policy(&mut self, var: InferVar) -> Option<VarPolicy> {
        match self.vars.probe_value(VarKey(var)) {
            VarValue::Unsolved { policy, .. } => Some(policy),
            VarValue::Solved(_) => None,
        }
    }

    /// [`InferenceTable::new_var`] wrapped as a type.
    pub fn new_var_ty(&mut self) -> Ty {
        Ty::infer_var(self.new_var())
    }

    /// Returns the canonical representative when `var`'s equivalence class
    /// still lacks a solution.
    pub fn unsolved_root_var(&mut self, var: InferVar) -> Option<InferVar> {
        let root = self.vars.find(VarKey(var));
        matches!(self.vars.probe_value(root), VarValue::Unsolved { .. }).then_some(root.0)
    }

    /// Defaults every still-unsolved effect variable's class to `never`.
    /// Run before the finalize erasure so effects never become errors.
    pub fn default_unsolved_effects_to_never(&mut self) {
        let len = u32::try_from(self.vars.len())
            .unwrap_or_else(|_| unreachable!("ena keys are u32-indexed"));
        for index in 0..len {
            let key = VarKey(InferVar::new(index));
            if let VarValue::Unsolved { policy, .. } = self.vars.probe_value(key)
                && policy.defaults_to_never()
            {
                self.vars.union_value(key, VarValue::Solved(Ty::never()));
            }
        }
    }

    /// The fixpoint-tier slice of the effect default (rustc runs
    /// fallback at quiescence and fulfillment RE-RUNS after it): only
    /// effect classes with NO accumulated bounds default here - a
    /// bounded effect class still solves from its evidence once this
    /// default grounds it. Returns whether anything defaulted.
    pub fn default_unbounded_effects_to_never(&mut self) -> bool {
        // Bounds may be keyed under a non-root alias; compare by ROOT.
        let bound_keys: Vec<u32> = self
            .bounds
            .iter()
            .filter(|(_, bounds)| !bounds.lowers.is_empty() || !bounds.uppers.is_empty())
            .map(|(&key, _)| key)
            .collect();
        let bound_roots: Vec<VarKey> = bound_keys
            .into_iter()
            .map(|key| self.vars.find(VarKey(InferVar::new(key))))
            .collect();
        let mut any = false;
        let len = u32::try_from(self.vars.len())
            .unwrap_or_else(|_| unreachable!("ena keys are u32-indexed"));
        for index in 0..len {
            let key = VarKey(InferVar::new(index));
            let VarValue::Unsolved { policy, .. } = self.vars.probe_value(key) else {
                continue;
            };
            if !policy.defaults_to_never() {
                continue;
            }
            let root = self.vars.find(key);
            if bound_roots.contains(&root) {
                continue;
            }
            self.vars.union_value(root, VarValue::Solved(Ty::never()));
            any = true;
        }
        any
    }

    /// Replaces a solved variable at the ROOT of `ty` with its solution,
    /// repeatedly; never descends into children.
    pub fn shallow_resolve(&mut self, ty: &Ty) -> Ty {
        let mut ty = ty.clone();
        loop {
            let InferTy::InferVar { var, .. } = ty.kind() else {
                return ty;
            };
            match self.vars.probe_value(VarKey(*var)) {
                VarValue::Solved(solution) => ty = solution,
                VarValue::Unsolved { .. } => return ty,
            }
        }
    }

    /// Substitutes every solved variable in `ty`, at any depth. Unresolved
    /// variables remain as `Infer` nodes (`resolve_all` at finalization is
    /// what forbids them).
    pub fn resolve_completely(&mut self, ty: &Ty) -> Ty {
        if !ty.has_infer() {
            return ty.clone();
        }
        let ty = self.shallow_resolve(ty);
        if !ty.has_infer() {
            return ty;
        }
        Ty::intern(
            ty.kind()
                .map_children(|child| self.resolve_completely(child)),
        )
    }

    /// Eagerly unifies `left` and `right` structurally, solving variables.
    /// Symmetric; errors carry the innermost mismatching pair.
    pub fn unify(&mut self, left: &Ty, right: &Ty) -> Result<(), UnifyError> {
        let left = self.shallow_resolve(left);
        let right = self.shallow_resolve(right);
        // Interning makes structural equality pointer equality.
        if left == right {
            return Ok(());
        }
        // Error unifies with everything: a diagnostic was already emitted.
        if matches!(left.kind(), InferTy::Error { .. })
            || matches!(right.kind(), InferTy::Error { .. })
        {
            return Ok(());
        }
        match (left.kind(), right.kind()) {
            (InferTy::InferVar { var: a, .. }, InferTy::InferVar { var: b, .. }) => {
                let root_a = self.vars.find(VarKey(*a)).0.index();
                let root_b = self.vars.find(VarKey(*b)).0.index();
                self.vars.union(VarKey(*a), VarKey(*b));
                if root_a != root_b {
                    let merged_root = self.vars.find(VarKey(*a)).0.index();
                    let mut merged = self.bounds.remove(&root_a).unwrap_or_default();
                    let other = self.bounds.remove(&root_b).unwrap_or_default();
                    merged.lowers.extend(other.lowers);
                    merged.uppers.extend(other.uppers);
                    if !(merged.lowers.is_empty() && merged.uppers.is_empty()) {
                        self.bounds.insert(merged_root, merged);
                    }
                    // The merged class took the shallower universe; every
                    // variable its bounds carry is observable through it now.
                    self.generalize_bounds_into(InferVar::new(merged_root));
                }
                Ok(())
            }
            (InferTy::InferVar { var, .. }, _) => self.bind(*var, &right, &left),
            (_, InferTy::InferVar { var, .. }) => self.bind(*var, &left, &right),
            _ => self.unify_kinds(&left, &right),
        }
    }

    /// Runs `f` inside a snapshot: rolled back on `Err`, committed on `Ok` -
    /// the probing primitive method resolution and call checking build on.
    pub fn commit_if_ok<T, E>(
        &mut self,
        f: impl FnOnce(&mut InferenceTable) -> Result<T, E>,
    ) -> Result<T, E> {
        let snapshot = self.snapshot();
        match f(self) {
            Ok(value) => {
                self.commit(snapshot);
                Ok(value)
            }
            Err(err) => {
                self.rollback_to(snapshot);
                Err(err)
            }
        }
    }

    pub fn snapshot(&mut self) -> Snapshot {
        Snapshot {
            vars: self.vars.snapshot(),
            universe: self.universe,
            bounds: self.bounds.clone(),
        }
    }

    pub fn rollback_to(&mut self, snapshot: Snapshot) {
        debug_assert_eq!(
            self.universe, snapshot.universe,
            "a probe must leave the scope depth it found: nothing it relates \
             can outlive it, so an unbalanced scope would strand variables at \
             a depth no block will close"
        );
        self.vars.rollback_to(snapshot.vars);
        self.bounds = snapshot.bounds;
    }

    pub fn commit(&mut self, snapshot: Snapshot) {
        self.vars.commit(snapshot.vars);
    }

    /// Records a lower bound (a value flowing into the variable), deposited
    /// at `anchor`.
    pub fn add_lower_bound(&mut self, var: InferVar, ty: Ty, anchor: Option<ExprId>) {
        self.generalize_into(var, &ty);
        let root = self.vars.find(VarKey(var)).0.index();
        self.bounds
            .entry(root)
            .or_default()
            .lowers
            .push(Bound { ty, anchor });
    }

    /// Records an upper bound (a context the variable flows into),
    /// deposited at `anchor`.
    pub fn add_upper_bound(&mut self, var: InferVar, ty: Ty, anchor: Option<ExprId>) {
        self.generalize_into(var, &ty);
        let root = self.vars.find(VarKey(var)).0.index();
        self.bounds
            .entry(root)
            .or_default()
            .uppers
            .push(Bound { ty, anchor });
    }

    /// rustc's generalizer at a relation: once `ty` is a bound or the
    /// solution of `var`'s class, every open variable inside it is
    /// observable through that class, so it may take no block-scoped
    /// parameter the class may not. Each is lowered to the class's
    /// universe, and so is everything that can flow into it (the open
    /// variables in its own bounds, transitively): a value reaches an
    /// outer class through any of them, and the universe check at its
    /// eventual solve is what refuses the parameter, at the deposit's
    /// anchor. A var-var union keeps the shallower depth for the same
    /// reason. Universes only differ while some binding is open, so the
    /// walk is skipped otherwise.
    pub fn generalize_into(&mut self, var: InferVar, ty: &Ty) {
        if self.scoped_params.is_empty() {
            return;
        }
        let VarValue::Unsolved { universe, .. } = self.vars.probe_value(VarKey(var)) else {
            return;
        };
        let ty = self.resolve_completely(ty);
        let mut worklist = Vec::new();
        collect_infer_vars(&ty, &mut worklist);
        let mut visited = FxHashSet::default();
        while let Some(inner) = worklist.pop() {
            let root_key = self.vars.find(VarKey(inner));
            let root = root_key.0;
            if !visited.insert(root.index()) {
                continue;
            }
            let VarValue::Unsolved {
                universe: inner_universe,
                ..
            } = self.vars.probe_value(root_key)
            else {
                continue;
            };
            if inner_universe <= universe {
                continue;
            }
            self.demote_to(root, universe);
            if let Some(bounds) = self.bounds.get(&root.index()) {
                let carried: Vec<Ty> = bounds
                    .lowers
                    .iter()
                    .chain(bounds.uppers.iter())
                    .map(|bound| bound.ty.clone())
                    .collect();
                for bound in carried {
                    let bound = self.resolve_completely(&bound);
                    collect_infer_vars(&bound, &mut worklist);
                }
            }
        }
    }

    /// [`InferenceTable::generalize_into`] over every bound `var`'s class
    /// already carries: for a class whose universe just dropped.
    fn generalize_bounds_into(&mut self, var: InferVar) {
        if self.scoped_params.is_empty() {
            return;
        }
        let root = self.vars.find(VarKey(var)).0.index();
        let carried: Vec<Ty> = self
            .bounds
            .get(&root)
            .map(|bounds| {
                bounds
                    .lowers
                    .iter()
                    .chain(bounds.uppers.iter())
                    .map(|bound| bound.ty.clone())
                    .collect()
            })
            .unwrap_or_default();
        for bound in carried {
            self.generalize_into(var, &bound);
        }
    }

    /// The accumulated bounds of `var`'s equivalence class (empty when
    /// none). Structural-resolution input.
    pub fn var_bounds(&mut self, var: InferVar) -> VarBounds {
        let root = self.vars.find(VarKey(var)).0.index();
        self.bounds.get(&root).cloned().unwrap_or_default()
    }

    /// Bounds accumulated on classes that DIRECT unification has since
    /// solved, as `(solution, bounds)` pairs; the ledger entries are
    /// removed. Binding a variable must discharge its pending evidence by
    /// replay (the engine re-drives each bound against the solution),
    /// never by dropping it.
    pub fn take_solved_class_bounds(&mut self) -> Vec<(Ty, VarBounds)> {
        let roots: Vec<u32> = self.bounds.keys().copied().collect();
        let mut out = Vec::new();
        for root in roots {
            let key = VarKey(InferVar::new(root));
            if let VarValue::Solved(ty) = self.vars.probe_value(key) {
                let bounds = self.bounds.remove(&root).unwrap_or_default();
                out.push((ty, bounds));
            }
        }
        out
    }

    /// Every still-unsolved class that has accumulated bounds, as
    /// `(representative var, bounds)`. Resolution input.
    pub fn unsolved_bounded_vars(&mut self) -> Vec<(InferVar, VarBounds)> {
        let roots: Vec<u32> = self.bounds.keys().copied().collect();
        let mut out = Vec::new();
        for root in roots {
            let key = VarKey(InferVar::new(root));
            if matches!(self.vars.probe_value(key), VarValue::Unsolved { .. }) {
                let bounds = self.bounds.get(&root).cloned().unwrap_or_default();
                out.push((InferVar::new(root), bounds));
            }
        }
        out.sort_by_key(|(var, _)| var.index());
        out
    }

    /// Whether `var`'s class already carries a solution. Resolution
    /// passes iterate a var list collected up front, and a
    /// generalization step can ALIAS two listed vars mid-pass - the
    /// later var must be re-checked before acting on it (rustc's
    /// shallow-resolve-before-relating discipline).
    pub fn is_solved(&mut self, var: InferVar) -> bool {
        matches!(self.vars.probe_value(VarKey(var)), VarValue::Solved(_))
    }

    /// Solves `var := ty` directly (resolution-time binding; unlike
    /// [`InferenceTable::unify`] this performs no occurs check because the
    /// solution was derived from resolved bounds).
    pub fn solve(&mut self, var: InferVar, ty: Ty) {
        self.generalize_into(var, &ty);
        self.vars.union_value(VarKey(var), VarValue::Solved(ty));
    }

    /// Solves `var := ty` after the occurs check. `var_ty` is the variable as
    /// a type, for the error value.
    fn bind(&mut self, var: InferVar, ty: &Ty, var_ty: &Ty) -> Result<(), UnifyError> {
        if self.occurs(VarKey(var), ty) {
            return Err(UnifyError {
                left: var_ty.clone(),
                right: ty.clone(),
                escaped: None,
            });
        }
        if let Some(param) = self.escaping_scoped_param(var, ty) {
            return Err(UnifyError {
                left: var_ty.clone(),
                right: ty.clone(),
                escaped: Some(param),
            });
        }
        self.generalize_into(var, ty);
        self.vars
            .union_value(VarKey(var), VarValue::Solved(ty.clone()));
        Ok(())
    }

    /// Whether `root`'s equivalence class occurs anywhere inside `ty`
    /// (resolving through solved variables). Binding on occurrence would
    /// build an infinite type.
    fn occurs(&mut self, root: VarKey, ty: &Ty) -> bool {
        if !ty.has_infer() {
            return false;
        }
        if let InferTy::InferVar { var, .. } = ty.kind() {
            if self.vars.unioned(VarKey(*var), root) {
                return true;
            }
            let resolved = self.shallow_resolve(ty);
            if &resolved == ty {
                return false;
            }
            return self.occurs(root, &resolved);
        }
        let mut found = false;
        let mut children = Vec::new();
        for_each_child(ty.kind(), |child| children.push(child.clone()));
        for child in children {
            if self.occurs(root, &child) {
                found = true;
                break;
            }
        }
        found
    }

    /// Structural unification of two non-var, non-equal heads: same head with
    /// identical non-child payload recurses on children; anything else is a
    /// mismatch. Leaf pairs never match here - equal leaves were caught by
    /// pointer equality in [`InferenceTable::unify`].
    fn unify_kinds(&mut self, left: &Ty, right: &Ty) -> Result<(), UnifyError> {
        let mismatch = || UnifyError {
            left: left.clone(),
            right: right.clone(),
            escaped: None,
        };
        let pairs: Vec<(Ty, Ty)> = match (left.kind(), right.kind()) {
            (InferTy::Class(ln, la, lat), InferTy::Class(rn, ra, rat))
                if ln == rn && la.len() == ra.len() && lat == rat =>
            {
                la.iter().cloned().zip(ra.iter().cloned()).collect()
            }
            (InferTy::List(li, lat), InferTy::List(ri, rat)) if lat == rat => {
                vec![(li.clone(), ri.clone())]
            }
            (
                InferTy::Map {
                    key: lk,
                    value: lv,
                    attr: lat,
                },
                InferTy::Map {
                    key: rk,
                    value: rv,
                    attr: rat,
                },
            ) if lat == rat => {
                vec![(lk.clone(), rk.clone()), (lv.clone(), rv.clone())]
            }
            (InferTy::Future(lv, le, lat), InferTy::Future(rv, re, rat)) if lat == rat => {
                vec![(lv.clone(), rv.clone()), (le.clone(), re.clone())]
            }
            (InferTy::Union(lm, lat), InferTy::Union(rm, rat))
                if lm.len() == rm.len() && lat == rat =>
            {
                // Positional; the ACI (reorder/absorb) equality class defers
                // to the budgeted machinery that lands with Sub constraints.
                lm.iter().cloned().zip(rm.iter().cloned()).collect()
            }
            (InferTy::Interface(ln, la, lassoc, lat), InferTy::Interface(rn, ra, rassoc, rat))
                if ln == rn
                    && la.len() == ra.len()
                    && lassoc.len() == rassoc.len()
                    && lassoc
                        .iter()
                        .zip(rassoc.iter())
                        .all(|((lname, _), (rname, _))| lname == rname)
                    && lat == rat =>
            {
                la.iter()
                    .cloned()
                    .zip(ra.iter().cloned())
                    .chain(
                        lassoc
                            .iter()
                            .map(|(_, ty)| ty.clone())
                            .zip(rassoc.iter().map(|(_, ty)| ty.clone())),
                    )
                    .collect()
            }
            (
                InferTy::Function {
                    params: lp,
                    ret: lr,
                    throws: le,
                    attr: lat,
                },
                InferTy::Function {
                    params: rp,
                    ret: rr,
                    throws: re,
                    attr: rat,
                },
            ) if lp.len() == rp.len()
                && lp
                    .iter()
                    .zip(rp.iter())
                    .all(|(l, r)| l.name == r.name && l.mode == r.mode)
                && lat == rat =>
            {
                lp.iter()
                    .map(|p| p.ty.clone())
                    .zip(rp.iter().map(|p| p.ty.clone()))
                    .chain([(lr.clone(), rr.clone()), (le.clone(), re.clone())])
                    .collect()
            }
            _ => return Err(mismatch()),
        };
        for (l, r) in pairs {
            self.unify(&l, &r)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use baml_type::TyAttr;

    use super::*;

    #[test]
    fn fresh_vars_are_distinct_and_unsolved() {
        let mut table = InferenceTable::new();
        let a = table.new_var_ty();
        let b = table.new_var_ty();
        assert_ne!(a, b);
        assert_eq!(table.shallow_resolve(&a), a);
    }

    #[test]
    fn unsolved_root_var_tracks_equivalence_classes() {
        let mut table = InferenceTable::new();
        let a = table.new_var();
        let b = table.new_var();
        assert_ne!(table.unsolved_root_var(a), table.unsolved_root_var(b));

        table.unify(&Ty::infer_var(a), &Ty::infer_var(b)).unwrap();
        assert_eq!(table.unsolved_root_var(a), table.unsolved_root_var(b));

        table.unify(&Ty::infer_var(a), &Ty::int()).unwrap();
        assert_eq!(table.unsolved_root_var(a), None);
        assert_eq!(table.unsolved_root_var(b), None);
    }

    #[test]
    fn binding_a_var_solves_it() {
        let mut table = InferenceTable::new();
        let a = table.new_var_ty();
        table.unify(&a, &Ty::int()).unwrap();
        assert_eq!(table.shallow_resolve(&a), Ty::int());
    }

    #[test]
    fn var_var_union_shares_the_solution() {
        let mut table = InferenceTable::new();
        let a = table.new_var_ty();
        let b = table.new_var_ty();
        table.unify(&a, &b).unwrap();
        table.unify(&b, &Ty::string()).unwrap();
        assert_eq!(table.shallow_resolve(&a), Ty::string());
    }

    #[test]
    fn structural_unification_decomposes_and_solves() {
        let mut table = InferenceTable::new();
        let elem = table.new_var_ty();
        table
            .unify(&Ty::list(elem.clone()), &Ty::list(Ty::int()))
            .unwrap();
        assert_eq!(table.shallow_resolve(&elem), Ty::int());

        let err = table
            .unify(&Ty::list(Ty::int()), &Ty::list(Ty::string()))
            .unwrap_err();
        assert_eq!(
            err,
            UnifyError {
                left: Ty::int(),
                right: Ty::string(),
                escaped: None,
            }
        );
    }

    #[test]
    fn known_var_unifies_through_its_solution() {
        let mut table = InferenceTable::new();
        let a = table.new_var_ty();
        let b = table.new_var_ty();
        table.unify(&a, &Ty::int()).unwrap();
        table.unify(&b, &Ty::int()).unwrap();
        // Both known: relate the solutions, never merge known roots.
        table.unify(&a, &b).unwrap();
        assert!(table.unify(&a, &Ty::string()).is_err());
    }

    #[test]
    fn occurs_check_rejects_infinite_types() {
        let mut table = InferenceTable::new();
        let a = table.new_var_ty();
        assert!(table.unify(&a, &Ty::list(a.clone())).is_err());
        // Also through a chain: ?b := ?a, ?a = List(?b).
        let b = table.new_var_ty();
        table.unify(&a, &b).unwrap();
        assert!(table.unify(&a, &Ty::list(b.clone())).is_err());
    }

    #[test]
    fn error_unifies_with_anything() {
        let mut table = InferenceTable::new();
        let a = table.new_var_ty();
        table.unify(&Ty::error(), &Ty::int()).unwrap();
        table.unify(&a, &Ty::error()).unwrap();
        // The var stays unsolved rather than being poisoned with Error.
        assert_eq!(table.shallow_resolve(&a), a);
    }

    #[test]
    fn resolve_completely_folds_nested_solutions() {
        let mut table = InferenceTable::new();
        let elem = table.new_var_ty();
        let ty = Ty::list(Ty::union([Ty::int(), elem.clone()]));
        table.unify(&elem, &Ty::string()).unwrap();
        let resolved = table.resolve_completely(&ty);
        assert_eq!(resolved, Ty::list(Ty::union([Ty::int(), Ty::string()])));
        assert!(!resolved.has_infer());
    }

    fn class(name: &str, args: impl IntoIterator<Item = Ty>) -> Ty {
        use baml_type::{Name, TypeName};
        Ty::intern(InferTy::Class(
            TypeName::local(Name::new(name)),
            args.into_iter().collect(),
            TyAttr::default(),
        ))
    }

    fn map(key: Ty, value: Ty) -> Ty {
        Ty::intern(InferTy::Map {
            key,
            value,
            attr: TyAttr::default(),
        })
    }

    fn func(params: impl IntoIterator<Item = Ty>, ret: Ty, throws: Ty) -> Ty {
        use baml_type::interned::InferFunctionParamTy;
        Ty::intern(InferTy::Function {
            params: params
                .into_iter()
                .map(|ty| InferFunctionParamTy::required(None, ty))
                .collect(),
            ret,
            throws,
            attr: TyAttr::default(),
        })
    }

    #[test]
    fn vars_solve_through_deep_nesting() {
        // Map<?k, List<Box<?e>>> = Map<string, List<Box<int | null>>>
        let mut table = InferenceTable::new();
        let k = table.new_var_ty();
        let e = table.new_var_ty();
        let left = map(k.clone(), Ty::list(class("Box", [e.clone()])));
        let right = map(
            Ty::string(),
            Ty::list(class("Box", [Ty::union([Ty::int(), Ty::null()])])),
        );
        table.unify(&left, &right).unwrap();
        assert_eq!(table.shallow_resolve(&k), Ty::string());
        assert_eq!(
            table.shallow_resolve(&e),
            Ty::union([Ty::int(), Ty::null()])
        );
        assert_eq!(table.resolve_completely(&left), right);
    }

    #[test]
    fn union_members_solve_positionally() {
        // (int | ?a | ?b) = (int | string | bool), including a nested union
        // member: List(int | (string | ?x)) = List(int | (string | bool)).
        let mut table = InferenceTable::new();
        let a = table.new_var_ty();
        let b = table.new_var_ty();
        table
            .unify(
                &Ty::union([Ty::int(), a.clone(), b.clone()]),
                &Ty::union([Ty::int(), Ty::string(), Ty::bool()]),
            )
            .unwrap();
        assert_eq!(table.shallow_resolve(&a), Ty::string());
        assert_eq!(table.shallow_resolve(&b), Ty::bool());

        let x = table.new_var_ty();
        let nested_left = Ty::list(Ty::union([Ty::int(), Ty::union([Ty::string(), x.clone()])]));
        let nested_right = Ty::list(Ty::union([
            Ty::int(),
            Ty::union([Ty::string(), Ty::bool()]),
        ]));
        table.unify(&nested_left, &nested_right).unwrap();
        assert_eq!(table.shallow_resolve(&x), Ty::bool());
    }

    /// Pins the S5 limitation: ground unions unify positionally, so a
    /// reordered but ACI-equal union is a mismatch today. The budgeted ACI
    /// machinery that lands with Sub constraints (README, constraint-system
    /// decision) relaxes this; when it does, this test flips.
    #[test]
    fn reordered_unions_do_not_unify_yet() {
        let mut table = InferenceTable::new();
        assert!(
            table
                .unify(
                    &Ty::union([Ty::int(), Ty::string()]),
                    &Ty::union([Ty::string(), Ty::int()]),
                )
                .is_err()
        );
    }

    #[test]
    fn repeated_var_constrains_both_positions() {
        // Pair<?a, ?a> = Pair<int, ?b>: the second position relates the
        // now-solved ?a against ?b, so ?b := int.
        let mut table = InferenceTable::new();
        let a = table.new_var_ty();
        let b = table.new_var_ty();
        table
            .unify(
                &class("Pair", [a.clone(), a]),
                &class("Pair", [Ty::int(), b.clone()]),
            )
            .unwrap();
        assert_eq!(table.shallow_resolve(&b), Ty::int());
        // And a later contradiction on either alias is caught.
        assert!(table.unify(&b, &Ty::string()).is_err());
    }

    #[test]
    fn diamond_of_var_unions_resolves_from_one_binding() {
        let mut table = InferenceTable::new();
        let vars: Vec<Ty> = (0..6).map(|_| table.new_var_ty()).collect();
        // Chain pairs, then cross-link into a diamond, then bind one end.
        table.unify(&vars[0], &vars[1]).unwrap();
        table.unify(&vars[2], &vars[3]).unwrap();
        table.unify(&vars[4], &vars[5]).unwrap();
        table.unify(&vars[1], &vars[2]).unwrap();
        table.unify(&vars[3], &vars[4]).unwrap();
        table.unify(&vars[5], &Ty::never()).unwrap();
        for var in &vars {
            assert_eq!(table.shallow_resolve(var), Ty::never());
        }
    }

    #[test]
    fn var_bound_to_composite_containing_other_vars() {
        // ?b = List(?a); ?b = List(int) => ?a := int, and a type mentioning
        // ?b resolves through both hops.
        let mut table = InferenceTable::new();
        let a = table.new_var_ty();
        let b = table.new_var_ty();
        table.unify(&b, &Ty::list(a.clone())).unwrap();
        table.unify(&b, &Ty::list(Ty::int())).unwrap();
        assert_eq!(table.shallow_resolve(&a), Ty::int());
        let wrapper = map(Ty::string(), Ty::union([b.clone(), Ty::null()]));
        assert_eq!(
            table.resolve_completely(&wrapper),
            map(Ty::string(), Ty::union([Ty::list(Ty::int()), Ty::null()]))
        );
    }

    #[test]
    fn function_types_solve_params_ret_and_throws() {
        let mut table = InferenceTable::new();
        let p = table.new_var_ty();
        let r = table.new_var_ty();
        let e = table.new_var_ty();
        table
            .unify(
                &func([p.clone(), Ty::string()], r.clone(), e.clone()),
                &func([Ty::int(), Ty::string()], Ty::bool(), Ty::never()),
            )
            .unwrap();
        assert_eq!(table.shallow_resolve(&p), Ty::int());
        assert_eq!(table.shallow_resolve(&r), Ty::bool());
        assert_eq!(table.shallow_resolve(&e), Ty::never());
        // Arity mismatch is a head mismatch, not a partial solve.
        assert!(
            table
                .unify(
                    &func([Ty::int()], Ty::int(), Ty::never()),
                    &func([Ty::int(), Ty::int()], Ty::int(), Ty::never())
                )
                .is_err()
        );
    }

    #[test]
    fn occurs_check_rejects_deeply_buried_cycles() {
        // ?a = Map<string, List<(int | Box<?a>)>> must be rejected, however
        // deep the occurrence and even through an aliased var.
        let mut table = InferenceTable::new();
        let a = table.new_var_ty();
        let alias = table.new_var_ty();
        table.unify(&a, &alias).unwrap();
        let cyclic = map(
            Ty::string(),
            Ty::list(Ty::union([Ty::int(), class("Box", [alias.clone()])])),
        );
        assert!(table.unify(&a, &cyclic).is_err());
    }

    #[test]
    fn hundred_levels_of_nesting_unify_and_resolve() {
        let mut table = InferenceTable::new();
        let core = table.new_var_ty();
        let mut left = core.clone();
        let mut right = Ty::union([Ty::int(), Ty::string()]);
        let expected_core = right.clone();
        for _ in 0..100 {
            left = Ty::list(left);
            right = Ty::list(right);
        }
        table.unify(&left, &right).unwrap();
        assert_eq!(table.shallow_resolve(&core), expected_core);
        assert_eq!(table.resolve_completely(&left), right);
    }

    #[test]
    fn nested_probes_roll_back_independently() {
        let mut table = InferenceTable::new();
        let a = table.new_var_ty();
        let b = table.new_var_ty();
        let outer: Result<(), ()> = table.commit_if_ok(|table| {
            table.unify(&a, &Ty::int()).map_err(|_| ())?;
            // Inner probe fails and must roll back only its own work.
            let inner: Result<(), ()> = table.commit_if_ok(|table| {
                table.unify(&b, &Ty::string()).map_err(|_| ())?;
                Err(())
            });
            assert!(inner.is_err());
            assert_eq!(table.shallow_resolve(&b), b, "inner rollback");
            assert_eq!(table.shallow_resolve(&a), Ty::int(), "outer intact");
            Ok(())
        });
        outer.unwrap();
        assert_eq!(table.shallow_resolve(&a), Ty::int());
        assert_eq!(table.shallow_resolve(&b), b);
    }

    #[test]
    fn resolution_is_idempotent_and_identity_on_var_free_types() {
        let mut table = InferenceTable::new();
        let e = table.new_var_ty();
        let ty = Ty::list(Ty::union([Ty::int(), e.clone()]));
        table.unify(&e, &Ty::string()).unwrap();
        let once = table.resolve_completely(&ty);
        let twice = table.resolve_completely(&once);
        assert_eq!(once, twice);
        // Var-free input returns the same interned handle, no rebuild.
        let ground = map(Ty::string(), Ty::list(Ty::bool()));
        assert_eq!(table.resolve_completely(&ground), ground);
    }

    #[test]
    fn rollback_reverts_var_kinds_with_their_indices() {
        // A rolled-back probe frees its variable indices for reuse. The
        // kind must be freed WITH the index: when it lived in side tables,
        // the fresh value variable below inherited the dead effect var's
        // kind and was silently defaulted to `never`.
        let mut table = InferenceTable::new();
        let snapshot = table.snapshot();
        let _effect = table.new_var_ty_of(VarPolicy::Effect);
        table.rollback_to(snapshot);

        let value = table.new_var_ty();
        table.default_unsolved_effects_to_never();
        assert_eq!(
            table.shallow_resolve(&value),
            value,
            "a value variable reusing a rolled-back effect var's index must not default"
        );

        let snapshot = table.snapshot();
        let _slot = table.new_var_ty_of(VarPolicy::ContainerSlot);
        table.rollback_to(snapshot);
        let plain = table.new_var();
        assert_eq!(table.unsolved_policy(plain), Some(VarPolicy::Value));
    }

    #[test]
    fn policy_joins_over_unions_and_retires_at_solution() {
        let mut table = InferenceTable::new();
        let slot = table.new_var_ty_of(VarPolicy::ContainerSlot);
        let plain = table.new_var_ty();
        // Unioning a plain var into a container-slot class adopts the
        // class's policy (`Value` is the join identity).
        table.unify(&slot, &plain).unwrap();
        let InferTy::InferVar { var: plain_var, .. } = plain.kind().clone() else {
            unreachable!("fresh var");
        };
        assert_eq!(
            table.unsolved_policy(plain_var),
            Some(VarPolicy::ContainerSlot)
        );
        // Solving retires the policy: a solved class IS its solution.
        table.unify(&plain, &Ty::int()).unwrap();
        assert_eq!(table.unsolved_policy(plain_var), None);
        // A rollback of the solving step restores it, policy included.
        let mut table = InferenceTable::new();
        let slot = table.new_var_ty_of(VarPolicy::ContainerSlot);
        let InferTy::InferVar { var, .. } = slot.kind().clone() else {
            unreachable!("fresh var");
        };
        let snapshot = table.snapshot();
        table.unify(&slot, &Ty::int()).unwrap();
        assert_eq!(table.unsolved_policy(var), None);
        table.rollback_to(snapshot);
        assert_eq!(table.unsolved_policy(var), Some(VarPolicy::ContainerSlot));
    }

    #[test]
    fn lambda_param_joining_container_slot_takes_the_stronger_policy() {
        // `let xs = []; xs.push(x)` inside a lambda: the element slot and
        // the parameter var union, and the class must keep BOTH behaviors
        // (first-demand order AND unknown absorption).
        let mut table = InferenceTable::new();
        let param = table.new_var_ty_of(VarPolicy::LambdaParam);
        let slot = table.new_var_ty_of(VarPolicy::ContainerSlot);
        table.unify(&param, &slot).unwrap();
        let InferTy::InferVar { var, .. } = param.kind().clone() else {
            unreachable!("fresh var");
        };
        let policy = table.unsolved_policy(var).expect("still open");
        assert_eq!(policy, VarPolicy::ContainerSlot);
        assert!(policy.first_demand_commits());
        assert!(policy.absorbs_unknown());
    }

    #[test]
    fn a_scoped_parameter_solves_only_variables_minted_in_its_scope() {
        let mut table = InferenceTable::new();
        let outer = table.new_var();
        let param = ParamTy::new(
            super::super::SCOPED_PARAM_BIT | 7,
            baml_type::Name::new("T"),
        );
        let scoped = Ty::intern(InferTy::TypeVar(param.clone(), TyAttr::default()));

        table.bind_scoped_param(&param);
        let inner = table.new_var();
        // Minted inside the binding's scope: may take the parameter.
        table.unify(&Ty::infer_var(inner), &scoped).unwrap();
        assert_eq!(table.shallow_resolve(&Ty::infer_var(inner)), scoped);
        // Minted outside: may not, even under a constructor, and stays open.
        let err = table
            .unify(&Ty::infer_var(outer), &Ty::list(scoped.clone()))
            .unwrap_err();
        assert_eq!(err.escaped.as_ref(), Some(&param));
        assert!(table.unsolved_root_var(outer).is_some());
        assert_eq!(
            table.escaping_scoped_param(outer, &Ty::list(scoped.clone())),
            Some(param.clone())
        );
        assert_eq!(table.escaping_scoped_param(inner, &scoped), None);

        // A var-var union is observable from the shallower scope, so the
        // merged class takes the shallower universe.
        let inner_alias = table.new_var();
        table
            .unify(&Ty::infer_var(inner_alias), &Ty::infer_var(outer))
            .unwrap();
        assert_eq!(
            table.escaping_scoped_param(inner_alias, &scoped),
            Some(param.clone())
        );

        // After the block closes, a fresh variable is outer again, and the
        // closed parameter still counts as escaping.
        table.close_scopes_to(0);
        let later = table.new_var();
        assert_eq!(
            table.escaping_scoped_param(later, &scoped),
            Some(param.clone())
        );
        // A rollback restores the universe with the rest of the var state.
        let snapshot = table.snapshot();
        table.unify(&Ty::infer_var(later), &Ty::int()).unwrap();
        table.rollback_to(snapshot);
        assert_eq!(table.escaping_scoped_param(later, &scoped), Some(param));
    }

    #[test]
    fn a_closing_block_leaves_its_open_variables_to_the_enclosing_universe() {
        let mut table = InferenceTable::new();
        let param = ParamTy::new(
            super::super::SCOPED_PARAM_BIT | 9,
            baml_type::Name::new("T"),
        );
        let scoped = Ty::intern(InferTy::TypeVar(param.clone(), TyAttr::default()));
        let before = table.new_var();
        table.bind_scoped_param(&param);
        let inner = table.new_var();
        let solved = table.new_var();
        assert_eq!(table.escaping_scoped_param(inner, &scoped), None);
        assert_eq!(
            table.escaping_scoped_param(before, &scoped),
            Some(param.clone())
        );
        assert_eq!(table.unsolved_vars_deeper_than(0), vec![inner, solved]);

        // Closing: a class the block leaves open moves out, and from there
        // the closed parameter is an escape for it too.
        table.close_scopes_to(0);
        table.demote_to(inner, 0);
        assert_eq!(table.unsolved_vars_deeper_than(0), vec![solved]);
        assert_eq!(
            table.escaping_scoped_param(inner, &scoped),
            Some(param.clone())
        );
        // Demotion never lifts a class deeper or touches a solved one.
        table.demote_to(before, 0);
        assert_eq!(table.escaping_scoped_param(before, &scoped), Some(param));
        table.unify(&Ty::infer_var(solved), &Ty::int()).unwrap();
        table.demote_to(solved, 0);
        assert!(table.is_solved(solved));
        assert!(table.unsolved_vars_deeper_than(0).is_empty());
    }

    #[test]
    fn a_deposit_generalizes_the_variables_it_carries_to_the_receiving_class() {
        let mut table = InferenceTable::new();
        let outer = table.new_var();
        let outer_union = table.new_var();
        let param = ParamTy::new(
            super::super::SCOPED_PARAM_BIT | 3,
            baml_type::Name::new("T"),
        );
        let scoped = Ty::intern(InferTy::TypeVar(param.clone(), TyAttr::default()));

        table.bind_scoped_param(&param);
        let inner = table.new_var();
        let carried = table.new_var();
        table.add_lower_bound(inner, Ty::infer_var(carried), None);
        // Minted inside the binding's scope: both may take the parameter.
        assert_eq!(table.escaping_scoped_param(inner, &scoped), None);
        assert_eq!(table.escaping_scoped_param(carried, &scoped), None);

        // Depositing `inner[]` into the outer class makes `inner` observable
        // from outside, and through its own bound so is `carried`: neither may
        // take the parameter any more (rustc's generalizer).
        table.add_lower_bound(outer, Ty::list(Ty::infer_var(inner)), None);
        assert_eq!(
            table.escaping_scoped_param(inner, &scoped),
            Some(param.clone())
        );
        assert_eq!(
            table.escaping_scoped_param(carried, &scoped),
            Some(param.clone())
        );

        // A SOLUTION carries its variables out the same way a bound does.
        let via_solution = table.new_var();
        assert_eq!(table.escaping_scoped_param(via_solution, &scoped), None);
        table.solve(outer, Ty::list(Ty::infer_var(via_solution)));
        assert_eq!(
            table.escaping_scoped_param(via_solution, &scoped),
            Some(param.clone())
        );

        // So does a var-var union, which takes the shallower depth: the merged
        // class's own bounds are generalized with it.
        let holder = table.new_var();
        let deep = table.new_var();
        table.add_upper_bound(holder, Ty::infer_var(deep), None);
        assert_eq!(table.escaping_scoped_param(deep, &scoped), None);
        table
            .unify(&Ty::infer_var(outer_union), &Ty::infer_var(holder))
            .unwrap();
        assert_eq!(
            table.escaping_scoped_param(deep, &scoped),
            Some(param.clone())
        );

        // Closing the scope RETIRES the parameter: a variable minted after it,
        // at the very depth a sibling block reuses, still cannot take it.
        table.close_scopes_to(0);
        let sibling_depth = table.new_var();
        table.bind_scoped_param(&ParamTy::new(
            super::super::SCOPED_PARAM_BIT | 4,
            baml_type::Name::new("U"),
        ));
        let in_sibling_block = table.new_var();
        assert_eq!(
            table.escaping_scoped_param(sibling_depth, &scoped),
            Some(param.clone())
        );
        assert_eq!(
            table.escaping_scoped_param(in_sibling_block, &scoped),
            Some(param)
        );
    }

    #[test]
    fn rollback_undoes_and_commit_keeps() {
        let mut table = InferenceTable::new();
        let a = table.new_var_ty();

        let snapshot = table.snapshot();
        table.unify(&a, &Ty::int()).unwrap();
        table.rollback_to(snapshot);
        assert_eq!(table.shallow_resolve(&a), a, "rollback must unsolve");

        let outcome: Result<(), ()> =
            table.commit_if_ok(|table| table.unify(&a, &Ty::string()).map_err(|_| ()));
        outcome.unwrap();
        assert_eq!(table.shallow_resolve(&a), Ty::string(), "commit must keep");

        let failed: Result<(), ()> = table.commit_if_ok(|table| {
            let b = table.new_var_ty();
            table.unify(&b, &Ty::int()).map_err(|_| ())?;
            Err(())
        });
        assert!(failed.is_err());
        assert_eq!(table.shallow_resolve(&a), Ty::string());
    }
}
