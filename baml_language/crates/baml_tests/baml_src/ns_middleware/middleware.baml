// Tests for BEP-034 `with` middleware: `spawn ... with` transformer pipelines
// over `baml.spawn.SpawnParams`.
//
// Converted from crates/bex_engine/tests/middleware.rs. Each Rust test's
// `function main()` becomes a top-level helper named after the test — the VM
// boxes `test`-block locals (a local compares unequal to a literal), so every
// value under test is produced by a function call and asserted on directly.
// Transformers shared by several Rust tests (`withDouble`) are declared once.

// ─── identity_transformer ─────────────────────────────────────────────────────

// The simplest transformer: identity. Exercises the chain plumbing
// (`SpawnParams` construction, transformer application, engine dispatch from
// the final params).
function ident<T, E>() -> (baml.spawn.SpawnParams<T, E>) -> baml.spawn.SpawnParams<T, E> throws never {
    (params) -> { params }
}

function identity_transformer() -> int {
    let f = spawn with ident() { 41 + 1 };
    await f
}

test "identity_transformer" {
    assert.equal(identity_transformer(), 42)
}

// ─── options_as_transformer ───────────────────────────────────────────────────

// `baml.spawn.options(...)` is an ordinary transformer.
function options_as_transformer() -> int {
    let f = spawn "opted" with baml.spawn.options(detach = false) { 7 };
    await f
}

test "options_as_transformer" {
    assert.equal(options_as_transformer(), 7)
}

// ─── body_wrapping_transformer ────────────────────────────────────────────────

// A transformer that WRAPS the body — the runtime must run the wrapped body
// from the final params, not the original closure operand. Shared with
// `chained_transformers_apply_left_to_right` and
// `variable_bound_transformer_still_applies`, which use the same transformer.
function withDouble<E>() -> (baml.spawn.SpawnParams<int, E>) -> baml.spawn.SpawnParams<int, E> throws never {
    (params) -> {
        let original = params.body;
        baml.spawn.SpawnParams {
            body: () -> { original() * 2 },
            name: params.name,
            group: params.group,
            cancel: params.cancel,
            detach: params.detach,
        }
    }
}

function body_wrapping_transformer() -> int {
    let f = spawn with withDouble() { 10 };
    await f
}

test "body_wrapping_transformer" {
    assert.equal(body_wrapping_transformer(), 20)
}

// ─── chained_transformers_apply_left_to_right ─────────────────────────────────

function withAddOne<E>() -> (baml.spawn.SpawnParams<int, E>) -> baml.spawn.SpawnParams<int, E> throws never {
    (params) -> {
        let original = params.body;
        baml.spawn.SpawnParams {
            body: () -> { original() + 1 },
            name: params.name,
            group: params.group,
            cancel: params.cancel,
            detach: params.detach,
        }
    }
}

// Pipeline order is left-to-right: each transformer wraps the body the previous
// one produced, so the LAST one's wrap runs outermost.
// `withDouble, withAddOne` → addOne(double(10)) = 21 (not 22).
function chained_transformers_apply_left_to_right() -> int {
    let f = spawn with withDouble(), withAddOne() { 10 };
    await f
}

test "chained_transformers_apply_left_to_right" {
    assert.equal(chained_transformers_apply_left_to_right(), 21)
}

// ─── custom_with_retry ────────────────────────────────────────────────────────

function flaky_then_42(calls: int[]) -> int throws string {
    calls.push(1);
    if (calls.length() < 3) {
        throw "flaky"
    }
    42
}

// The BEP's marquee example: user-written retry middleware. The body fails
// twice then succeeds; `withRetry(5)` re-runs it until it does.
function withRetry<T, E>(n: int) -> (baml.spawn.SpawnParams<T, E>) -> baml.spawn.SpawnParams<T, E> throws never {
    (params) -> {
        let original = params.body;
        baml.spawn.SpawnParams {
            body: () -> {
                let winners: T[] = [];
                let attempts = 0;
                while (winners.length() == 0 && attempts < n - 1) {
                    attempts = attempts + 1;
                    (winners.push(original())) catch (e) {
                        let e => {}
                    };
                }
                if (winners.length() > 0) {
                    winners[0]
                } else {
                    original()
                }
            },
            name: params.name,
            group: params.group,
            cancel: params.cancel,
            detach: params.detach,
        }
    }
}

function custom_with_retry() -> int {
    let calls: int[] = [];
    let f = spawn "retry-test" with withRetry(5) { flaky_then_42(calls) };
    await f
}

test "custom_with_retry" {
    assert.equal(custom_with_retry(), 42)
}

// ─── type_changing_fallback_transformer ───────────────────────────────────────

// A TYPE-CHANGING transformer (the BEP's `withFallback`): wraps the body with a
// catch-all, so the error type is erased — the spawn types `Future<int, never>`
// and the failing body resolves to the default.
function withFallback<T, E>(default_value: T) -> (baml.spawn.SpawnParams<T, E>) -> baml.spawn.SpawnParams<T, never> throws never {
    (params) -> {
        let original = params.body;
        baml.spawn.SpawnParams {
            body: () -> {
                (original()) catch (e) {
                    let e => default_value
                }
            },
            name: params.name,
            group: params.group,
            cancel: params.cancel,
            detach: params.detach,
        }
    }
}

function flaky(n: int) -> int throws string {
    if (n > 0) {
        throw "boom"
    }
    n
}

function type_changing_fallback_transformer() -> int {
    let f = spawn with withFallback(99) { flaky(1) };
    // `f: Future<int, never>` — awaiting needs no catch.
    await f
}

test "type_changing_fallback_transformer" {
    assert.equal(type_changing_fallback_transformer(), 99)
}

// ─── throwing_transformer_throws_at_spawn_site ────────────────────────────────

// The pipeline runs transformers EAGERLY in the spawner, so a transformer that
// throws does so at the spawn site — catchable there, and part of the CALLER's
// throws surface (an explicit `throws never` caller is rejected; see the TIR
// throws contribution).
function withBomb<T, E>() -> (baml.spawn.SpawnParams<T, E>) -> baml.spawn.SpawnParams<T, E> throws string throws never {
    (params) -> { throw "cfg-exploded" }
}

function throwing_transformer_throws_at_spawn_site() -> string {
    let r = (spawn with withBomb() { 1 }) catch (e) {
        let s: string => s
    };
    // The body never ran; the transformer's throw surfaced at spawn.
    match (r) {
        let s: string => s,
        _ => "wrong"
    }
}

test "throwing_transformer_throws_at_spawn_site" {
    assert.equal(throwing_transformer_throws_at_spawn_site(), "cfg-exploded")
}

// ─── variable_bound_transformer_still_applies ─────────────────────────────────

// A transformer bound to a VARIABLE first (not called inline). The `let` binding
// gives generic inference no context to solve `E`, so it is supplied explicitly
// (`withDouble<never>()` — the spawn body throws nothing, so its `SpawnParams`
// error type is `never`); the pipeline still runs from the variable.
function variable_bound_transformer_still_applies() -> int {
    let t = withDouble<never>();
    let f = spawn with t { 21 };
    await f
}

test "variable_bound_transformer_still_applies" {
    assert.equal(variable_bound_transformer_still_applies(), 42)
}
