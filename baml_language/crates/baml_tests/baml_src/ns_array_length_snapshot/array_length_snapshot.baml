// Ensures `let n = a.length()` snapshots the length before later mutations.
//
// Uses a top-level helper because the VM has a local-boxing quirk (locals
// cannot live directly in test blocks; see ns_bigints/bigints.baml).

function length_snapshot_bug() -> int {
    let a: int[] = [];
    let n = a.length();
    a.push(9);
    n
}

test "length_binding_snapshots_before_mutation" {
    assert.equal(length_snapshot_bug(), 0)
}
