function leaf() -> int { 7 }

function main() -> int {
    let child = spawn { leaf() };
    leaf() + (await child)
}

test "cloud telemetry execution" {
    assert.equal(main(), 14)
}
