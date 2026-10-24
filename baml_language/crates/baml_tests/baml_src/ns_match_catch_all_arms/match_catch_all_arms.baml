// An arm that only binds (`_`, `let whole`) asks nothing of its scrutinee's
// structure: it takes the value as it is, so a `match` made only of such arms
// is typed before the scrutinee's type is settled, and what settles it may
// come later.

function mca_wildcard_before_the_push() -> int {
    let xs = [];
    let n = match (xs) {
        _ => 0,
    };
    xs.push(1);
    xs.length() + n
}

test "a wildcard-only match leaves an open scrutinee to be settled later" {
    assert.equal(mca_wildcard_before_the_push(), 1)
}

function mca_binding_before_the_push() -> int {
    let xs = [];
    let first = match (xs) {
        let whole => whole,
    };
    xs.push("s");
    first.length() + xs.length()
}

test "a binding arm takes the open scrutinee as it is" {
    assert.equal(mca_binding_before_the_push(), 2)
}

function mca_guarded(x: int) -> string {
    let xs = [];
    let kind = match (xs) {
        _ if x > 0 => "positive",
        let whole => "other",
    };
    xs.push(x);
    kind
}

test "a guarded catch-all falls through to the next" {
    assert.equal(mca_guarded(1), "positive");
    assert.equal(mca_guarded(-1), "other")
}
