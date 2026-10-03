// `all` collects every value in input order on the happy path.

function main() -> int {
    let fs = [spawn { 1 }, spawn { 2 }, spawn { 3 }];
    let r = await baml.future.all(fs);
    r[0] * 100 + r[1] * 10 + r[2]
}

test "all_collects_in_order" {
    assert.equal(main(), 123)
}
