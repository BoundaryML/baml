// BUG 1: `x = expr ?? x` assigned null (MIR pre-wrote the target before the RHS).
function bug1() -> int {
    let a: int? = 3;
    let n: int? = null;
    a = n ?? a;      // pre-fix: a becomes null; post-fix: stays 3
    a ?? -1
}

test "bug1_self_assign_keeps_value" {
    assert.equal(bug1(), 3)
}
