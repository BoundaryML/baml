//! Ordered extension points. First matching lowering wins; errors never fall through.
use super::{
    BTreeMap, Db, Emitter, Gen, MirFunction, Plan, Repr, Rvalue, SourceRoot, StatementKind,
    Terminator, calls, closures, containers, dispatch, globals, loop_ranges, strings, tasks,
    telemetry,
};

#[derive(Clone, Copy)]
pub(super) enum Admission<'a, 'db> {
    Initializers(bool),
    Function(&'a MirFunction<'db>),
    CapturedLocal(bool),
    ShieldedBlock(bool),
    Statement(&'a StatementKind<'db>),
    Class { generic: bool, cleanup: bool },
}

pub(super) trait Feature {
    // Register initializer/lambda/specialized MIR and any additional roots before
    // the reachability walk. Lifetime-only generics keep this trait object-safe.
    fn prepare<'db>(
        &self,
        _db: &'db dyn Db,
        _root: SourceRoot,
        _plan: &mut Plan<'_>,
        _all: &mut BTreeMap<usize, MirFunction<'db>>,
    ) -> Gen<Vec<usize>> {
        Ok(Vec::new())
    }

    fn call(
        &self,
        _emitter: &Emitter<'_, '_>,
        _name: &str,
        _types: &[Repr],
        _values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        None
    }

    fn admit(&self, _event: &Admission<'_, '_>) -> Gen<()> {
        Ok(())
    }
    fn value(
        &self,
        _emitter: &Emitter<'_, '_>,
        _value: &Rvalue<'_>,
        _expected: Repr,
    ) -> Option<Gen<(String, Repr)>> {
        None
    }
    fn statement(
        &self,
        _emitter: &mut Emitter<'_, '_>,
        _statement: &StatementKind<'_>,
    ) -> Option<Gen<String>> {
        None
    }
    fn terminator(
        &self,
        _emitter: &mut Emitter<'_, '_>,
        _term: &Terminator<'_>,
    ) -> Option<Gen<String>> {
        None
    }
    // A claimed terminator returns its reachable function objects in stable order.
    fn reach(
        &self,
        _db: &dyn Db,
        _plan: &mut Plan<'_>,
        _term: Option<&Terminator<'_>>,
    ) -> Option<Gen<Reach>> {
        None
    }
}

const FEATURES: &[&dyn Feature] = &[
    &globals::Feature,
    &closures::Feature,
    &tasks::Feature,
    &containers::Feature,
    &dispatch::Feature,
    &super::interface_fields::Feature,
    &super::switches::Feature,
    &strings::Feature,
    &telemetry::Feature,
    &loop_ranges::Feature,
    &super::generics::Feature,
    &calls::Feature,
];

pub(super) fn admit(event: Admission<'_, '_>) -> Gen<()> {
    for feature in FEATURES {
        feature.admit(&event)?;
    }
    Ok(())
}
pub(super) fn value(
    emitter: &Emitter<'_, '_>,
    value: &Rvalue<'_>,
    expected: Repr,
) -> Option<Gen<(String, Repr)>> {
    FEATURES
        .iter()
        .find_map(|f| f.value(emitter, value, expected))
}
pub(super) fn statement(
    emitter: &mut Emitter<'_, '_>,
    statement: &StatementKind<'_>,
) -> Option<Gen<String>> {
    FEATURES
        .iter()
        .find_map(|f| f.statement(emitter, statement))
}
pub(super) fn terminator(
    emitter: &mut Emitter<'_, '_>,
    term: &Terminator<'_>,
) -> Option<Gen<String>> {
    FEATURES.iter().find_map(|f| f.terminator(emitter, term))
}
pub(super) fn reach(
    db: &dyn Db,
    plan: &mut Plan<'_>,
    term: Option<&Terminator<'_>>,
) -> Option<Gen<Reach>> {
    FEATURES.iter().find_map(|f| f.reach(db, plan, term))
}

pub(super) fn call(
    emitter: &Emitter<'_, '_>,
    name: &str,
    types: &[Repr],
    values: &[(String, Repr)],
) -> Option<Gen<(String, Repr)>> {
    FEATURES
        .iter()
        .find_map(|f| f.call(emitter, name, types, values))
}

#[derive(Default)]
pub(super) struct Reach {
    pub targets: Vec<usize>,
    pub builtin: Option<String>,
    pub json_decode: Option<usize>,
}

pub(super) fn prepare<'db>(
    db: &'db dyn Db,
    root: SourceRoot,
    plan: &mut Plan<'_>,
    all: &mut BTreeMap<usize, MirFunction<'db>>,
) -> Gen<Vec<usize>> {
    let mut roots = Vec::new();
    for feature in FEATURES {
        roots.extend(feature.prepare(db, root, plan, all)?);
    }
    Ok(roots)
}
