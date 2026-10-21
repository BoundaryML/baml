// `all_settled` awaits every future and returns their outcomes in input order.

function main() -> int {
    let fs = [spawn { 1 }, spawn { 2 }, spawn { 3 }];
    let outcomes = await baml.future.all_settled(fs);
    let results = outcomes.map((outcome) -> {
        match (outcome) {
            let s: baml.future.Success<int> => s.value,
            _ => baml.sys.panic("expected success"),
        }
    });
    results[0] * 100 + results[1] * 10 + results[2]
}

test "all_settled_collects_in_order" {
    assert.equal(main(), 123)
}
