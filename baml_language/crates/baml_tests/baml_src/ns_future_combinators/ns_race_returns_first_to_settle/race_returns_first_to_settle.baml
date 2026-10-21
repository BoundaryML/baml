// `race` settles with the FIRST future to settle — here the faster one wins.

function slow() -> int {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(300n));
    2
}

function fast() -> int {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(20n));
    1
}

function main() -> int {
    let fs = [spawn { slow() }, spawn { fast() }];
    await baml.future.race(fs)
}

test "race_returns_first_to_settle" {
    assert.equal(main(), 1)
}
