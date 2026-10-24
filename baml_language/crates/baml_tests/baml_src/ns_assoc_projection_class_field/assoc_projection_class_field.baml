// A class field typed by an associated-type projection on the class's own
// parameter: emit resolves the declaring interface from the parameter's
// bound, exactly as the checker does.

interface ApcfHasOut {
    type Out
}

class ApcfIntOut {
    implements ApcfHasOut {
        type Out = int
    }
}

class ApcfProj<T extends ApcfHasOut> {
    v T.Out
}

test "a class field typed by a projection on the class parameter" {
    let p: ApcfProj<ApcfIntOut> = ApcfProj { v: 3 };
    assert.equal(p.v, 3)
}

class ApcfBoolOut {
    implements ApcfHasOut {
        type Out = bool
    }
}

function apcf_flag(p: ApcfProj<ApcfBoolOut>) -> int {
    match (p) {
        ApcfProj<ApcfBoolOut> { v: true } => 1,
        ApcfProj<ApcfBoolOut> { v: false } => 0,
    }
}

test "patterns on a projection-typed field see the type it reduces to" {
    let yes: ApcfProj<ApcfBoolOut> = ApcfProj { v: true };
    let no: ApcfProj<ApcfBoolOut> = ApcfProj { v: false };
    assert.equal(apcf_flag(yes), 1);
    assert.equal(apcf_flag(no), 0)
}
