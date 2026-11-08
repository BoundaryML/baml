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
    let Object::Class(class) = plan.program.objects.get(object)? else {
        return None;
    };
    let package = plan
        .program
        .packages
        .iter()
        .find(|p| p.objects.contains(&(object as u32)))?;
    let name = format!("{}.{}", package.name, class.path);
    Some(match name.as_str() {
        "baml.panics.IntegerOverflow" => 1,
        "baml.panics.NegativeBitShift" => 2,
        "baml.panics.Cancelled" => 3,
        "baml.panics.StackOverflow" => 4,
        "baml.panics.AllocFailure" => 5,
        "baml.panics.IndexOutOfBounds" => 6,
        "baml.json.ParseError" => 7,
        "baml.json.DecodeError" => 8,
        "baml.panics.DivisionByZero" => 9,
        "baml.errors.InvalidArgument" => 10,
        "baml.errors.ParseError" => 11,
        "baml.panics.HostUnavailable" => 12,
        "baml.errors.Io" => 13,
        "baml.panics.MapKeyNotFound" => 14,
        _ => return None,
    })
}
