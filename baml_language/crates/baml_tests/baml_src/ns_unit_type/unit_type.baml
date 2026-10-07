// BEP-079: `void` and `null` are two spellings of one unit type.
//
// The compile errors this rules out live in
// `projects/diagnostic_errors/unit_type`.

class UnitLog {
    entries: string[],
}

function unit_side_effect(log: UnitLog) -> void {
    log.entries.push("ran");
}

function unit_as_void() -> void {}

function unit_as_null() -> null {
    null
}

// ---- every call has a usable result ----------------------------------------

test "a void call's result is the unit value" {
    let log = UnitLog { entries: [] };
    let result = unit_side_effect(log);
    assert.equal(result, null);
    assert.equal(log.entries, ["ran"])
}

// ---- the two spellings are one type ------------------------------------------

// Containers are invariant and function types are compared exactly in these
// positions, so each of these compiles only if `void` and `null` are the same
// type rather than merely assignable to each other.
function unit_list_of_void_is_list_of_null() -> null[] {
    let xs: void[] = [unit_as_void(), unit_as_null()];
    xs
}

function unit_map_of_null_is_map_of_void(m: map<string, null>) -> map<string, void> {
    m
}

function unit_callbacks(fs: (() -> void throws never)[]) -> (() -> null throws never)[] {
    fs
}

test "void and null are one type in invariant positions" {
    assert.equal(unit_list_of_void_is_list_of_null(), [null, null]);
    assert.equal(unit_map_of_null_is_map_of_void({ "k": null }), { "k": null });
    let fs = unit_callbacks([unit_as_void, unit_as_null]);
    assert.equal(fs.length(), 2);
    assert.equal(fs[0](), null)
}

// ---- and one type at run time ----------------------------------------------------

// A value carries its own concrete type, so with the static type widened to
// `unknown` these hold only if a `void` function *is* a `null` function.
test "a void function value is a null function value at run time" {
    let as_void: unknown = unit_as_void;
    let as_null: unknown = unit_as_null;
    assert.is_true(as_void is () -> null throws never);
    assert.is_true(as_null is () -> void throws never);
    assert.equal(reflect.Type.of_value(as_void).to_string(), "() -> null throws never");
    assert.equal(reflect.Type.of_value(as_null).to_string(), "() -> null throws never")
}

// A `test` body is a unit function, and `Package.tests()` declares its values
// as `() -> null throws unknown`: each one must be a member of that type.
test "a compiled package's tests are members of their declared type" {
    let pkg = reflect.Package.compile({ "main.baml": `test "t" { assert.equal(1, 1) }` });
    let tests = pkg.tests();
    let run: unknown = tests["root::t"];
    assert.is_true(run is () -> null throws unknown);
    if let typed: () -> null throws unknown = tests.get("root::t") {
        assert.equal(typed(), null)
    } else {
        throw "the test was not found under its declared type"
    }
}

// ---- `void` is a type like any other -------------------------------------------

type UnitAlias = void

class UnitHolder {
    marker: void,
    maybe: void?,
    many: void[],
}

function unit_param(x: void) -> UnitAlias {
    x
}

function unit_identity<T>(x: T) -> T {
    x
}

function unit_in_union(x: int | void) -> string {
    match (x) {
        null => "unit",
        int => "int",
    }
}

test "void is valid in every type position" {
    let holder = UnitHolder { marker: null, maybe: null, many: [null] };
    assert.equal(holder.marker, null);
    assert.equal(holder.many.length(), 1);
    assert.equal(unit_param(null), null);
    assert.equal(unit_identity<void>(null), null);
    assert.equal(unit_in_union(null), "unit");
    assert.equal(unit_in_union(3), "int")
}

// ---- the unit shorthands -----------------------------------------------------

function unit_bare_return(log: UnitLog, early: bool) -> null {
    if (early) {
        return;
    }
    log.entries.push("late");
}

test "return; and an omitted final expression are shorthand for null" {
    let log = UnitLog { entries: [] };
    assert.equal(unit_bare_return(log, true), null);
    assert.equal(unit_bare_return(log, false), null);
    assert.equal(log.entries, ["late"])
}

// ---- implicit units are values where nothing else is expected ---------------------

// With no expectation on it, a block with no final expression and an `if`
// with no `else` simply have type `null`; from there on it is an ordinary
// value, and `null <: int?`.
function unit_block_value(log: UnitLog) -> int? {
    let nothing = { log.entries.push("block"); };
    nothing
}

function unit_if_value(log: UnitLog, run: bool) -> int? {
    let nothing = if (run) { log.entries.push("if"); };
    nothing
}

test "a block with no final expression and an if with no else evaluate to null" {
    let log = UnitLog { entries: [] };
    assert.equal(unit_block_value(log), null);
    assert.equal(unit_if_value(log, true), null);
    assert.equal(unit_if_value(log, false), null);
    assert.equal(log.entries, ["block", "if"])
}

// ---- a position nobody observes still discards --------------------------------------

// Statement expressions, loop bodies and `defer` bodies are not read, so a
// value left there is dropped without a `;` — `push` returns the new length.
function unit_statement_positions(xs: int[], log: UnitLog) -> int {
    defer { log.entries.push("deferred") }
    for (let x in xs) { log.entries.push(`loop ${x}`) }
    if (xs.length() > 0) { log.entries.push("if") }
    xs.length()
}

test "values in statement positions are discarded" {
    let log = UnitLog { entries: [] };
    assert.equal(unit_statement_positions([1, 2], log), 2);
    assert.equal(log.entries, ["loop 1", "loop 2", "if", "deferred"])
}

// ---- the magic `cleanup` finalizer accepts either spelling ---------------------------

class UnitNullCleanup {
    log: string[],

    function cleanup(self) -> null {
        self.log.push("cleaned");
    }
}

test "cleanup declared with a null return is the finalizer and runs once" {
    let r = UnitNullCleanup { log: [] };
    r.cleanup();
    r.cleanup();
    assert.equal(r.log, ["cleaned"])
}

type UnitFinalizerReturn = void

class UnitAliasCleanup {
    log: string[],

    function cleanup(self) -> UnitFinalizerReturn {
        self.log.push("cleaned");
    }
}

test "cleanup declared with an alias of the unit type is the finalizer too" {
    let r = UnitAliasCleanup { log: [] };
    r.cleanup();
    r.cleanup();
    assert.equal(r.log, ["cleaned"])
}

// ---- a join of nothing but unit values is itself unit ---------------------------------

function unit_join(log: UnitLog, c: bool) -> null {
    let joined = if (c) { null } else { log.entries.push("else"); };
    let early = (stop: bool) -> {
        if (stop) { return; }
        log.entries.push("ran");
    };
    early(c);
    joined
}

test "implicit units join each other and an explicit null" {
    let log = UnitLog { entries: [] };
    assert.equal(unit_join(log, true), null);
    assert.equal(unit_join(log, false), null);
    assert.equal(log.entries, ["else", "ran"])
}

// ---- a `${…}` with no value runs, and hands a tag nothing ---------------------------------

//baml:tagged_string
function unit_tag(body: () -> baml.TaggedString) -> unknown[] {
    body().values
}

function unit_interpolations(log: UnitLog, run: bool) -> unknown[] {
    unit_tag`a${ log.entries.push("block"); }b${ if (run) { log.entries.push("if"); } }c${ 7 }`
}

function unit_interpolations_looped(log: UnitLog, times: int) -> unknown[] {
    unit_tag`${for (let i = 0; i < times; i += 1)}${ log.entries.push(`loop ${i}`); }${endfor}`
}

// A binding made by one is in scope for the segments after it.
function unit_interpolation_binding(xs: int[]) -> unknown[] {
    unit_tag`${ let n = xs.length() }${n} of ${xs}`
}

//baml:tagged_string
function unit_parts(body: () -> baml.TaggedString) -> string[] {
    body().parts
}

test "a tag receives nothing for an interpolation with no value" {
    let log = UnitLog { entries: [] };
    assert.equal(unit_interpolations(log, true), [7]);
    assert.equal(unit_interpolations(log, false), [7]);
    assert.equal(unit_interpolations_looped(log, 2), []);
    assert.equal(log.entries, ["block", "if", "block", "loop 0", "loop 1"]);
    assert.equal(unit_interpolation_binding([4, 5]), [2, [4, 5]]);
    // The text on either side of it joins.
    assert.equal(unit_parts`a${ log.entries.push("x"); }b${ 1 }c`, ["ab", "c"])
}

// ---- a header comment does not decide what a block evaluates to -----------------------------

function unit_header_after_value() -> int {
    let x = 4;
    x + 1
    //# done
}

function unit_header_after_statement(log: UnitLog) -> void {
    log.entries.push("ran");
    //# done
}

test "a header comment after the final expression leaves it the block's value" {
    let log = UnitLog { entries: [] };
    assert.equal(unit_header_after_value(), 5);
    unit_header_after_statement(log);
    assert.equal(log.entries, ["ran"])
}

// ---- only a session submission's final expression is its value ---------------------------

test "an earlier expression of a session submission is a statement" {
    let session = reflect.Session.new();
    session.eval(`let xs = [1, 2, 3]`);
    assert.equal(
        session.eval<int>(`
            if (xs.length() > 2) { xs.length() }
            xs.length()
        `),
        3,
    );
    assert.equal(
        session.eval<int>(`
            xs.length() + 1
            //# done
        `),
        4,
    )
}
