// docs:start all
function route_for(tokens: int) -> string {
    // docs:start choice
    let model = if (tokens <= 8_000) {
        "mini"
    } else if (tokens <= 100_000) {
        "standard"
    } else {
        "long-context"
    };
    // docs:end choice

    `route to ${model}`
}
// docs:end all

function main() -> string { route_for(12_400) }

test "chapter example" {
    assert.equal(main(), "route to standard")
}

test "routing boundaries" {
    assert.equal(route_for(8000), "route to mini");
    assert.equal(route_for(8001), "route to standard");
    assert.equal(route_for(100000), "route to standard");
    assert.equal(route_for(100001), "route to long-context")
}
