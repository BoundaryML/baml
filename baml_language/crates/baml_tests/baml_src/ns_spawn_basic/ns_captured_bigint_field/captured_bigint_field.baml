// A spawned closure can add a bigint field from its captured instance.
// Opcode assertions for spawn captures remain in spawn_specialization.rs.

class Config {
    page_budget_ms: bigint,
}

function broken(config: Config) -> bigint {
    let pending = spawn { config.page_budget_ms + 1n };
    await pending
}

function main() -> bigint {
    broken(Config { page_budget_ms: 10n })
}

test "spawned_closure_can_add_captured_bigint_field" {
    assert.equal(main(), 11n)
}
