// docs:start strings
function describe_cost(cents: int) -> string {
    `cost in cents: ${cents}`
}
// docs:end strings

// docs:start block
function remaining_budget() -> int {
    // docs:start block-expression
    let remaining = {
        let limit = 8_000; // Block-local
        limit - 512 // Block value: 7488
    };
    // docs:end block-expression
    remaining
}
// docs:end block

// docs:start early-return
function budget(stop: bool) -> int {
    let remaining = {
        if (stop) { return 0; } // Exits budget
        8_000 - 512             // Block value
    };
    remaining + 512             // Continues after the block
}
// docs:end early-return

test "return inside a block exits the enclosing function" {
    assert.equal(budget(true), 0);
    assert.equal(budget(false), 8000)
}

test "numeric operations used in the chapter" {
    assert.equal(285 / 100, 2);
    assert.equal(285 % 100, 85);
    assert.equal(285 / 100.0, 2.85);
    assert.equal(2 == 2.0, false);
    assert.equal(0.1 + 0.2, 0.30000000000000004);
    assert.equal((2.85).iround(), 3);
    assert.equal(2.pow(10), 1024)
}

test "text and block values" {
    assert.equal(describe_cost(285), "cost in cents: 285");
    assert.equal("cost in cents: ${cents}", "cost in cents: ${cents}");
    assert.equal("a" + "b", "ab");
    assert.equal(remaining_budget(), 7488)
}

test "const currently aliases let" {
    const remaining = 8000;
    remaining = 4000;
    assert.equal(remaining, 4000)
}
