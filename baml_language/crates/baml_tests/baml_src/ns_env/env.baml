// Tests for baml.env namespace.
//
// BAML has no `baml.env.set` — the stdlib exposes only `baml.env.get`,
// `baml.env.get_or_panic` and `baml.env.ref` (read-only).

// ============================================================================
// env_get_or_panic_missing_var
//
// get_or_panic on a key that is never set must throw baml.panics.UserPanic.
// The Rust test first calls remove_var to guarantee absence; we rely on the
// test key being absent in the CI environment (it is intentionally obscure).
// ============================================================================

test "env_get_or_panic_missing_var" {
    {
        baml.env.get_or_panic("BAML_TEST_MISSING_PANIC");
        baml.sys.panic("expected get_or_panic to throw")
    } catch (e) {
        baml.panics.UserPanic => null
    }
}

// ============================================================================
// env_get_missing_var_returns_null
//
// get() on an absent key returns null (string?).
// ============================================================================

test "env_get_missing_var_returns_null" {
    assert.equal(baml.env.get("BAML_TEST_NONEXISTENT_VAR"), null)
}

// ============================================================================
// env_sugar_missing_var
//
// The `env.KEY` sugar desugars to `baml.env.ref("KEY")` — a late-bound
// reference carrying the NAME only. It never reads the environment, so an
// absent key does not throw; the read happens through the reference.
// (`.get_or_panic()` is still the throwing read — see ns_env_ref_desugar for
// the full reference surface.)
// ============================================================================

test "env_sugar_missing_var" {
    assert.equal(env.BAML_TEST_SUGAR_MISSING.get(), null)
}
