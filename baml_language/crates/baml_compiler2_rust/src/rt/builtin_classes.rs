//! Runtime-produced exception objects and generated typed catches share these
//! descriptors. A catch must never mint a second tag for the same class.
use super::*;

pub(super) fn record_test(
    db: &dyn Db,
    plan: &mut Plan<'_>,
    statement: &StatementKind<'_>,
) -> Gen<()> {
    if let StatementKind::Assign {
        value:
            Rvalue::IsType {
                test: baml_compiler2_mir::TypeTest::Class { class, args },
                ..
            },
        ..
    } = statement
        && args.is_empty()
        && let Some((object, _)) = plan.index.class(db, *class)
        && reserved(plan, object).is_some()
    {
        // A typed catch need not bind the class to a typed MIR local. Its
        // type test and subsequent field reads still need the reserved layout.
        plan.add(object)?;
    }
    Ok(())
}

pub(super) fn reserved(plan: &Plan<'_>, object: usize) -> Option<usize> {
    plan.list_error_descriptor(object)
}
