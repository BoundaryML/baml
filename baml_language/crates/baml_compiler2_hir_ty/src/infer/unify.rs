//! The inference table: an ena union-find over [`InferVar`]s with
//! snapshot/rollback and eager, occurs-checked `Eq` unification - the
//! rust-analyzer `InferenceTable` shape, per the constraint-system design in
//! this crate's README.
//!
//! S5 scope: equality only. The settled `VarData` bounds
//! (lowers/uppers/obligations for `Sub` constraints and the obligation
//! worklist) join with the first `Sub` constraints; until then a variable's
//! class is solved or not (`VarValue`). Kind/policy metadata for variables
//! (effect vars, diverging vars) also lives here when it arrives - the
//! representation carries identity only.
//!
//! Unification discipline (rustc's `TypeVariableValue` model): both sides are
//! shallow-resolved before relating, so two `Known` roots never merge inside
//! ena's pure value-merge - a known root is unified structurally against the
//! other side instead. `Error` unifies with everything (a diagnostic was
//! already emitted; never cascade). Unions unify positionally for now: the
//! ACI-equality cases (reordered/var-bearing unions in invariant positions)
//! are the deferred-with-budget class that arrives with `Sub` constraints.

use baml_type::interned::{InferVar, Ty, TyKind, for_each_child};
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

/// Solver state of a variable's equivalence class.
#[derive(Debug, Clone, PartialEq)]
enum VarValue {
    Unknown,
    Known(Ty),
}

impl ut::UnifyValue for VarValue {
    type Error = ut::NoError;

    fn unify_values(a: &VarValue, b: &VarValue) -> Result<VarValue, ut::NoError> {
        match (a, b) {
            (VarValue::Known(_), VarValue::Known(_)) => unreachable!(
                "unify shallow-resolves before relating, so two known roots never merge"
            ),
            (VarValue::Known(ty), _) | (_, VarValue::Known(ty)) => Ok(VarValue::Known(ty.clone())),
            (VarValue::Unknown, VarValue::Unknown) => Ok(VarValue::Unknown),
        }
    }
}

/// A structural mismatch: the innermost pair of types that failed to unify.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnifyError {
    pub left: Ty,
    pub right: Ty,
}

/// Sub-constraint evidence accumulated on an unsolved variable's class:
/// lower bounds are values flowing INTO the variable, upper bounds are
/// contexts it flows into. Resolution derives the solution from these
/// (widen fresh lowers, lowers must agree, checked against the uppers).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VarBounds {
    pub lowers: Vec<Ty>,
    pub uppers: Vec<Ty>,
}

/// A revertible point in the table's history; see
/// [`InferenceTable::snapshot`]. The bounds map is snapshotted by clone
/// (rust-analyzer snapshots its fulfillment context the same way); the ena
/// undo log covers only the union-find.
pub struct Snapshot {
    vars: ut::Snapshot<ut::InPlace<VarKey>>,
    bounds: FxHashMap<u32, VarBounds>,
}

#[derive(Default)]
pub struct InferenceTable {
    vars: ut::InPlaceUnificationTable<VarKey>,
    /// Bounds per CLASS, keyed by the root's index; var-var unions merge the
    /// two roots' entries.
    bounds: FxHashMap<u32, VarBounds>,
    /// Creation indices of EFFECT variables (the throws channel). Their
    /// finalize default differs: an unconstrained effect is `never` -
    /// BAML's only defaulting rule (S12) - where an unconstrained value
    /// variable is an error (ruling 2).
    effect_vars: FxHashSet<u32>,
    /// Element/key/value variables of EMPTY container literals (the
    /// honest replacement for TIR's Evolving sentinels). These follow
    /// TIR's establishment-order rule when demands disagree: the first
    /// ground demand commits and later incompatible ones report at
    /// their own sites, where an ordinary var (a call instantiation)
    /// fails resolution instead (ruling 1).
    establishment_vars: FxHashSet<u32>,
}

impl InferenceTable {
    pub fn new() -> InferenceTable {
        InferenceTable::default()
    }

    /// Allocates a fresh, unconstrained inference variable.
    pub fn new_var(&mut self) -> InferVar {
        self.vars.new_key(VarValue::Unknown).0
    }

    /// [`InferenceTable::new_var`] wrapped as a type.
    pub fn new_var_ty(&mut self) -> Ty {
        Ty::infer_var(self.new_var())
    }

    /// [`InferenceTable::new_var`] for an empty container literal's
    /// element/key/value slot: solves establishment-order on
    /// disagreeing demands (see `establishment_vars`).
    pub fn new_establishment_var_ty(&mut self) -> Ty {
        let var = self.new_var();
        self.establishment_vars.insert(var.index());
        Ty::infer_var(var)
    }

    /// Returns the canonical representative when `var`'s equivalence class
    /// still lacks a solution.
    pub fn unsolved_root_var(&mut self, var: InferVar) -> Option<InferVar> {
        let root = self.vars.find(VarKey(var));
        matches!(self.vars.probe_value(root), VarValue::Unknown).then_some(root.0)
    }

    /// Whether `var`'s equivalence class contains an establishment var.
    pub fn is_establishment_var(&mut self, var: InferVar) -> bool {
        let root = self.vars.find(VarKey(var));
        let indices: Vec<u32> = self.establishment_vars.iter().copied().collect();
        indices
            .into_iter()
            .any(|index| self.vars.find(VarKey(InferVar::new(index))) == root)
    }

    /// An EFFECT variable: identical to a value variable except at
    /// finalize, where unconstrained effects default to `never`.
    pub fn new_effect_var_ty(&mut self) -> Ty {
        let var = self.new_var();
        self.effect_vars.insert(var.index());
        Ty::infer_var(var)
    }

    /// Defaults every still-unsolved effect variable's class to `never`.
    /// Run before the finalize erasure so effects never become errors.
    pub fn default_unsolved_effects_to_never(&mut self) {
        let indices: Vec<u32> = self.effect_vars.iter().copied().collect();
        for index in indices {
            let root = self.vars.find(VarKey(InferVar::new(index)));
            if matches!(self.vars.probe_value(root), VarValue::Unknown) {
                self.vars.union_value(root, VarValue::Known(Ty::never()));
            }
        }
    }

    /// The fixpoint-tier slice of the effect default (rustc runs
    /// fallback at quiescence and fulfillment RE-RUNS after it): only
    /// effect classes with NO accumulated bounds default here - a
    /// bounded effect class still solves from its evidence once this
    /// default grounds it. Returns whether anything defaulted.
    pub fn default_unbounded_effects_to_never(&mut self) -> bool {
        let indices: Vec<u32> = self.effect_vars.iter().copied().collect();
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
        for index in indices {
            let root = self.vars.find(VarKey(InferVar::new(index)));
            if !matches!(self.vars.probe_value(root), VarValue::Unknown) {
                continue;
            }
            if bound_roots.contains(&root) {
                continue;
            }
            self.vars.union_value(root, VarValue::Known(Ty::never()));
            any = true;
        }
        any
    }

    /// Replaces a solved variable at the ROOT of `ty` with its solution,
    /// repeatedly; never descends into children.
    pub fn shallow_resolve(&mut self, ty: &Ty) -> Ty {
        let mut ty = ty.clone();
        loop {
            let TyKind::Infer { var, .. } = ty.kind() else {
                return ty;
            };
            match self.vars.probe_value(VarKey(*var)) {
                VarValue::Known(solution) => ty = solution,
                VarValue::Unknown => return ty,
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
        if matches!(left.kind(), TyKind::Error { .. })
            || matches!(right.kind(), TyKind::Error { .. })
        {
            return Ok(());
        }
        match (left.kind(), right.kind()) {
            (TyKind::Infer { var: a, .. }, TyKind::Infer { var: b, .. }) => {
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
                }
                Ok(())
            }
            (TyKind::Infer { var, .. }, _) => self.bind(*var, &right, &left),
            (_, TyKind::Infer { var, .. }) => self.bind(*var, &left, &right),
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
            bounds: self.bounds.clone(),
        }
    }

    pub fn rollback_to(&mut self, snapshot: Snapshot) {
        self.vars.rollback_to(snapshot.vars);
        self.bounds = snapshot.bounds;
    }

    pub fn commit(&mut self, snapshot: Snapshot) {
        self.vars.commit(snapshot.vars);
    }

    /// Records a lower bound (a value flowing into the variable).
    pub fn add_lower_bound(&mut self, var: InferVar, ty: Ty) {
        let root = self.vars.find(VarKey(var)).0.index();
        self.bounds.entry(root).or_default().lowers.push(ty);
    }

    /// Records an upper bound (a context the variable flows into).
    pub fn add_upper_bound(&mut self, var: InferVar, ty: Ty) {
        let root = self.vars.find(VarKey(var)).0.index();
        self.bounds.entry(root).or_default().uppers.push(ty);
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
            if let VarValue::Known(ty) = self.vars.probe_value(key) {
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
            if self.vars.probe_value(key) == VarValue::Unknown {
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
        self.vars.probe_value(VarKey(var)) != VarValue::Unknown
    }

    /// Solves `var := ty` directly (resolution-time binding; unlike
    /// [`InferenceTable::unify`] this performs no occurs check because the
    /// solution was derived from resolved bounds).
    pub fn solve(&mut self, var: InferVar, ty: Ty) {
        self.vars.union_value(VarKey(var), VarValue::Known(ty));
    }

    /// Solves `var := ty` after the occurs check. `var_ty` is the variable as
    /// a type, for the error value.
    fn bind(&mut self, var: InferVar, ty: &Ty, var_ty: &Ty) -> Result<(), UnifyError> {
        if self.occurs(VarKey(var), ty) {
            return Err(UnifyError {
                left: var_ty.clone(),
                right: ty.clone(),
            });
        }
        self.vars
            .union_value(VarKey(var), VarValue::Known(ty.clone()));
        Ok(())
    }

    /// Whether `root`'s equivalence class occurs anywhere inside `ty`
    /// (resolving through solved variables). Binding on occurrence would
    /// build an infinite type.
    fn occurs(&mut self, root: VarKey, ty: &Ty) -> bool {
        if !ty.has_infer() {
            return false;
        }
        if let TyKind::Infer { var, .. } = ty.kind() {
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
        };
        let pairs: Vec<(Ty, Ty)> = match (left.kind(), right.kind()) {
            (TyKind::Class(ln, la, lat), TyKind::Class(rn, ra, rat))
                if ln == rn && la.len() == ra.len() && lat == rat =>
            {
                la.iter().cloned().zip(ra.iter().cloned()).collect()
            }
            (TyKind::List(li, lat), TyKind::List(ri, rat)) if lat == rat => {
                vec![(li.clone(), ri.clone())]
            }
            (
                TyKind::Map {
                    key: lk,
                    value: lv,
                    attr: lat,
                },
                TyKind::Map {
                    key: rk,
                    value: rv,
                    attr: rat,
                },
            ) if lat == rat => {
                vec![(lk.clone(), rk.clone()), (lv.clone(), rv.clone())]
            }
            (TyKind::Future(lv, le, lat), TyKind::Future(rv, re, rat)) if lat == rat => {
                vec![(lv.clone(), rv.clone()), (le.clone(), re.clone())]
            }
            (TyKind::Union(lm, lat), TyKind::Union(rm, rat))
                if lm.len() == rm.len() && lat == rat =>
            {
                // Positional; the ACI (reorder/absorb) equality class defers
                // to the budgeted machinery that lands with Sub constraints.
                lm.iter().cloned().zip(rm.iter().cloned()).collect()
            }
            (TyKind::Interface(ln, la, lassoc, lat), TyKind::Interface(rn, ra, rassoc, rat))
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
                TyKind::Function {
                    params: lp,
                    ret: lr,
                    throws: le,
                    attr: lat,
                },
                TyKind::Function {
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
                right: Ty::string()
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
        Ty::intern(TyKind::Class(
            TypeName::local(Name::new(name)),
            args.into_iter().collect(),
            TyAttr::default(),
        ))
    }

    fn map(key: Ty, value: Ty) -> Ty {
        Ty::intern(TyKind::Map {
            key,
            value,
            attr: TyAttr::default(),
        })
    }

    fn func(params: impl IntoIterator<Item = Ty>, ret: Ty, throws: Ty) -> Ty {
        use baml_type::interned::FunctionParam;
        Ty::intern(TyKind::Function {
            params: params
                .into_iter()
                .map(|ty| FunctionParam::required(None, ty))
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
