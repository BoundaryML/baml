// Tests for the runtime type identity of a spawned future (BEP-034).
//
// A heap `Future` carries the `<T, E>` its spawn site was typed at, so `is` and
// `match` relate those arguments invariantly — the way `int[]` is not
// `string[]` — instead of admitting every future through the coarse FUTURE type
// tag. A body that statically cannot throw spells its error type `never`.
//
// Helpers live at top level rather than inline in the test blocks: `is` and
// `match` inspect a value's runtime type, and a boxed test-block local
// misreports (same caveat as ns_is_operator).

// ── 1. The value type discriminates ───────────────────────────────────────────

function int_future_is_int_future_fn() -> bool throws never {
    let f = spawn { 1 };
    let observed = f is baml.future.Future<int, never>;
    let _ = await f;
    observed
}

test "int_future_is_int_future" {
    assert.is_true(int_future_is_int_future_fn())
}

function int_future_is_string_future_fn() -> bool throws never {
    let f = spawn { 1 };
    let observed = f is baml.future.Future<string, never>;
    let _ = await f;
    observed
}

test "int_future_is_not_string_future" {
    assert.is_true(!int_future_is_string_future_fn())
}

// ── 2. The error type discriminates ───────────────────────────────────────────

function fail_io() -> int throws baml.errors.Io {
    throw baml.errors.Io { message: "x" }
}

// Same value type as `spawn { 1 }`, different error type: the future must not
// pass a `Future<int, never>` test.
function throwing_future_is_non_throwing_future_fn() -> bool throws never {
    let f = spawn { fail_io() };
    let observed = f is baml.future.Future<int, never>;
    let _ = (await f) catch (e) { baml.errors.Io => 0 };
    observed
}

test "throwing_future_is_not_non_throwing_future" {
    assert.is_true(!throwing_future_is_non_throwing_future_fn())
}

// ── 3. A body that captures a local still reports the precise type ────────────

// The spawn body closes over `v`, so the value reaches the future through a
// capture cell rather than a literal in the body. The future's type is still the
// body's type, not the cell's.
function captured_local_future_is_int_future_fn() -> bool throws never {
    let v: int = 1;
    let f = spawn { v };
    let observed = f is baml.future.Future<int, never>;
    let _ = await f;
    observed
}

test "captured_local_future_is_int_future" {
    assert.is_true(captured_local_future_is_int_future_fn())
}

// ── 4. A spawn in a generic frame resolves against the caller's type args ─────

// The spawned body's type is the frame's `T`, so this future's value type is
// whatever `T` was specialized to at the call site — not a compile-time constant
// of this function. The spawn lowers `T` to a frame reference (`all_complete<T,
// E>` emits `spawn<#0[], #1>`; see the `__baml_std__` MIR snapshot) that the
// runtime resolves against the caller-supplied type args.
function spawn_is_int_future<T>(v: T) -> bool throws never {
    let f = spawn { v };
    let observed = f is baml.future.Future<int, never>;
    let _ = await f;
    observed
}

// Explicit type args at the call site.
function generic_spawn_at_int_fn() -> bool throws never {
    let v: int = 1;
    spawn_is_int_future<int>(v)
}

test "generic_spawn_at_int_is_int_future" {
    assert.is_true(generic_spawn_at_int_fn())
}

function generic_spawn_at_string_fn() -> bool throws never {
    let v: string = "a";
    spawn_is_int_future<string>(v)
}

test "generic_spawn_at_string_is_not_int_future" {
    assert.is_true(!generic_spawn_at_string_fn())
}

// The same pair with `T` INFERRED from the argument rather than written. This is
// the case that used to report `unknown`, so it is worth pinning separately from
// the explicit form above.
function inferred_generic_spawn_at_int_fn() -> bool throws never {
    let v: int = 1;
    spawn_is_int_future(v)
}

test "inferred_generic_spawn_at_int_is_int_future" {
    assert.is_true(inferred_generic_spawn_at_int_fn())
}

function inferred_generic_spawn_at_string_fn() -> bool throws never {
    let v: string = "a";
    spawn_is_int_future(v)
}

test "inferred_generic_spawn_at_string_is_not_int_future" {
    assert.is_true(!inferred_generic_spawn_at_string_fn())
}

// ── 5. `match` arms discriminate on the same footing ──────────────────────────

// No wildcard arm: the two arms target one union member each, so between them
// they exhaust the scrutinee. (Adding a `_` here is an `unreachable arm` error —
// which is itself the proof that the arms discriminate on the type arguments
// rather than each swallowing every future.)
function classify_future(
    f: baml.future.Future<int, never> | baml.future.Future<string, never>,
) -> string throws never {
    match (f) {
        let a: baml.future.Future<int, never> => "int",
        let b: baml.future.Future<string, never> => "string",
    }
}

function classify_int_future_fn() -> string throws never {
    let f = spawn { 1 };
    let observed = classify_future(f);
    let _ = await f;
    observed
}

test "match_picks_int_future_arm" {
    assert.equal(classify_int_future_fn(), "int")
}

function classify_string_future_fn() -> string throws never {
    let f = spawn { "a" };
    let observed = classify_future(f);
    let _ = await f;
    observed
}

test "match_picks_string_future_arm" {
    assert.equal(classify_string_future_fn(), "string")
}

// ── 6. Two future arms on the jump-table path ────────────────────────────────

// Bare (unbound) type arms at 4+ distinct tags lower to a tag-keyed jump table
// rather than the sequential chain. Every `Future<T, E>` carries the single
// FUTURE tag, so without a tag-sufficiency guard the two future arms dedup onto
// one slot and the first swallows the second — a `Future<string, never>` value
// would answer "int-future". Both conditions are load-bearing: `let a: T` arms
// encode as a chain, and fewer than four tags skips the table.
function classify_tagged(
    v: baml.future.Future<int, never> | baml.future.Future<string, never> | int | string | bool,
) -> string throws never {
    match (v) {
        baml.future.Future<int, never> => "int-future",
        baml.future.Future<string, never> => "string-future",
        int => "int",
        string => "string",
        bool => "bool",
    }
}

function tagged_string_future_fn() -> string throws never {
    let f = spawn { "a" };
    let observed = classify_tagged(f);
    let _ = await f;
    observed
}

test "jump_table_picks_string_future_arm" {
    assert.equal(tagged_string_future_fn(), "string-future")
}

function tagged_int_future_fn() -> string throws never {
    let f = spawn { 1 };
    let observed = classify_tagged(f);
    let _ = await f;
    observed
}

test "jump_table_picks_int_future_arm" {
    assert.equal(tagged_int_future_fn(), "int-future")
}

// ── 7. A future's arguments carry through a container element type ────────────

// `Future<T, E>[]` is how the combinators take their inputs
// (`baml.future.all_complete(futures)`), so the element type has to keep
// discriminating once it is a future.
function future_array_matches_fn() -> bool throws never {
    let fs: baml.future.Future<int, never>[] = [spawn { 1 }];
    let observed = fs is baml.future.Future<int, never>[];
    let _ = await fs[0];
    observed
}

test "future_array_matches_element_type" {
    assert.is_true(future_array_matches_fn())
}

function wrong_future_array_fn() -> bool throws never {
    let fs: baml.future.Future<string, never>[] = [spawn { "a" }];
    let observed = fs is baml.future.Future<int, never>[];
    let _ = await fs[0];
    observed
}

test "future_array_rejects_other_element_type" {
    assert.is_true(!wrong_future_array_fn())
}
