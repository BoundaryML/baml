// docs:start comments
/// Adds two integers.
// docs:end comments
// docs:start function
function add(
    left: int, right: int = 1
) -> int {
    left + right // Returned value
}
// docs:end function

function named_arguments() -> int {
    // docs:start arguments
    add(4);           // 5
    add(4, right = 2)  // 6
    // docs:end arguments
}

function sum(values: int[]) -> int {
    // docs:start loop
    let total = 0;
    for (let value in values) { // Values
        total = total + value;
    }
    // docs:end loop
    total
}

function choose_model(tokens: int) -> string {
    // docs:start condition
    let model = if (tokens <= 8_000) {
        "mini"
    } else {
        "standard"
    };
    // docs:end condition
    model
}

test "TypeScript comparison examples" {
    assert.equal(add(4), 5);
    assert.equal(add(4, right = 2), 6);
    assert.equal(named_arguments(), 6);
    assert.equal(sum([1, 2, 3]), 6);
    assert.equal(sum([]), 0);
    assert.equal(choose_model(8000), "mini");
    assert.equal(choose_model(8001), "standard")
}
