// Chained catches nest left to right: each clause protects the expression
// and handler bodies before it. Every clause has its own landing and panic
// policy. Arms within a single clause remain alternatives.

class NotFound {
    key: string,
}

function cc_lookup(n: int) -> int {
    match (n) {
        0 => throw NotFound { key: "k" },
        1 => throw "oops",
        _ => 42,
    }
}

function cc_index_panic() -> int {
    let xs = [1];
    xs[5]
}

// A `catch` wildcard before a `catch_all_panics` clause: the panic passes the
// first clause's `_` and reaches the second clause.
function cc_wildcard_then_catch_all_panics() -> string {
    cc_index_panic().to_string() catch (e) {
        _ => "first clause",
    } catch_all_panics (p) {
        _ => "panic clause",
    }
}

test "a panic passes a catch wildcard and reaches a later catch_all_panics clause" {
    assert.equal(cc_wildcard_then_catch_all_panics(), "panic clause")
}

// An error is still taken by the first clause's wildcard.
function cc_error_wildcard_then_catch_all_panics() -> string {
    cc_lookup(0).to_string() catch (e) {
        _ => "first clause",
    } catch_all_panics (p) {
        _ => "panic clause",
    }
}

test "an error is taken by the first clause's wildcard" {
    assert.equal(cc_error_wildcard_then_catch_all_panics(), "first clause")
}

// A panic passes a wildcard and reaches a named panic arm in a later clause.
function cc_wildcard_then_named_panic() -> string {
    cc_index_panic().to_string() catch (e) {
        _ => "wildcard",
    } catch (p) {
        baml.panics.IndexOutOfBounds => "named panic",
    }
}

test "a panic passes a wildcard and reaches a later named panic arm" {
    assert.equal(cc_wildcard_then_named_panic(), "named panic")
}

// A panic no arm takes still propagates past the whole chain.
function cc_unmatched_panic_propagates() -> string {
    {
        cc_index_panic().to_string() catch (e) {
            _ => "wildcard",
        } catch_all (e2) {
            _ => "catch_all wildcard",
        }
    } catch (outer) {
        baml.panics.IndexOutOfBounds => "outer",
    }
}

test "a panic no clause traps propagates past the chain" {
    assert.equal(cc_unmatched_panic_propagates(), "outer")
}

// `catch_all` covers what reaches it, so nothing propagates and a
// `throws never` contract holds.
function cc_catch_all_covers(n: int) -> int throws never {
    cc_lookup(n) catch_all (e) {
        NotFound => 1,
        string => 2,
    }
}

test "catch_all handles every error that reaches it" {
    assert.equal(cc_catch_all_covers(0) + cc_catch_all_covers(1) + cc_catch_all_covers(2), 45)
}

// A `catch_all` after a partial `catch` covers only the residual.
function cc_catch_then_catch_all(n: int) -> int throws never {
    cc_lookup(n) catch (e) {
        NotFound => 1,
    } catch_all (e2) {
        string => 2,
    }
}

test "catch_all after a partial catch covers the residual" {
    assert.equal(cc_catch_then_catch_all(0) + cc_catch_then_catch_all(1), 3)
}

// `catch_all_panics` covering its panics with a named alias arm.
function cc_catch_all_panics_named(n: int) -> string throws never {
    let r = {
        cc_lookup(n);
        cc_index_panic()
    } catch_all_panics (e) {
        NotFound => "not found",
        string => "string",
        baml.panics.Panic => "panic",
    };
    r.to_string()
}

test "catch_all_panics with a named panic arm" {
    assert.equal(cc_catch_all_panics_named(0), "not found");
    assert.equal(cc_catch_all_panics_named(1), "string");
    assert.equal(cc_catch_all_panics_named(2), "panic")
}

// A bare name or literal as the protected expression.
function cc_bare_name_base() -> int {
    let x = 1;
    x catch (e) {
        _ => 2,
    }
}

function cc_bare_literal_operand() -> int {
    1
        + 1 catch (e) {
            _ => 2,
        }
}

test "a bare name or literal can be the protected expression" {
    assert.equal(cc_bare_name_base(), 1);
    assert.equal(cc_bare_literal_operand(), 2)
}

// A `catch_all_panics` wildcard receives panics even after every error is
// handled, so its binding is typed as a panic, not `never`.
function cc_wildcard_binding_is_a_panic() -> string {
    cc_lookup(5).to_string() catch_all_panics (e) {
        _ => "unused",
    };
    cc_index_panic().to_string() catch_all_panics (e) {
        let p => {
            match (p) {
                let oob: baml.panics.IndexOutOfBounds => "index " + oob.index.to_string(),
                _ => "other panic",
            }
        },
    }
}

test "a catch_all_panics wildcard binding receives the panic" {
    assert.equal(cc_wildcard_binding_is_a_panic(), "index 5")
}

// The declared closed contract also checks that the later clause consumes
// the new error effect produced by the first handler.
function cc_arm_throw_catch_then_catch() -> int throws never {
    {
        throw "original"
    } catch (original) {
        _ => throw 42,
    } catch (replacement) {
        int => 7,
        baml.panics.Panic => -1,
    }
}

test "arm throw from catch reaches later catch" {
    assert.equal(cc_arm_throw_catch_then_catch(), 7)
}

// The declared closed contract also checks that the later clause consumes
// the new error effect produced by the first handler.
function cc_arm_throw_catch_then_catch_all() -> int throws never {
    {
        throw "original"
    } catch (original) {
        _ => throw 42,
    } catch_all (replacement) {
        int => 7,
        baml.panics.Panic => -1,
    }
}

test "arm throw from catch reaches later catch_all" {
    assert.equal(cc_arm_throw_catch_then_catch_all(), 7)
}

// The declared closed contract also checks that the later clause consumes
// the new error effect produced by the first handler.
function cc_arm_throw_catch_then_catch_all_panics() -> int throws never {
    {
        throw "original"
    } catch (original) {
        _ => throw 42,
    } catch_all_panics (replacement) {
        int => 7,
        baml.panics.Panic => -1,
    }
}

test "arm throw from catch reaches later catch_all_panics" {
    assert.equal(cc_arm_throw_catch_then_catch_all_panics(), 7)
}

// The declared closed contract also checks that the later clause consumes
// the new error effect produced by the first handler.
function cc_arm_throw_catch_all_then_catch() -> int throws never {
    {
        throw "original"
    } catch_all (original) {
        _ => throw 42,
    } catch (replacement) {
        int => 7,
        baml.panics.Panic => -1,
    }
}

test "arm throw from catch_all reaches later catch" {
    assert.equal(cc_arm_throw_catch_all_then_catch(), 7)
}

// The declared closed contract also checks that the later clause consumes
// the new error effect produced by the first handler.
function cc_arm_throw_catch_all_then_catch_all() -> int throws never {
    {
        throw "original"
    } catch_all (original) {
        _ => throw 42,
    } catch_all (replacement) {
        int => 7,
        baml.panics.Panic => -1,
    }
}

test "arm throw from catch_all reaches later catch_all" {
    assert.equal(cc_arm_throw_catch_all_then_catch_all(), 7)
}

// The declared closed contract also checks that the later clause consumes
// the new error effect produced by the first handler.
function cc_arm_throw_catch_all_then_catch_all_panics() -> int throws never {
    {
        throw "original"
    } catch_all (original) {
        _ => throw 42,
    } catch_all_panics (replacement) {
        int => 7,
        baml.panics.Panic => -1,
    }
}

test "arm throw from catch_all reaches later catch_all_panics" {
    assert.equal(cc_arm_throw_catch_all_then_catch_all_panics(), 7)
}

// The declared closed contract also checks that the later clause consumes
// the new error effect produced by the first handler.
function cc_arm_throw_catch_all_panics_then_catch() -> int throws never {
    {
        throw "original"
    } catch_all_panics (original) {
        _ => throw 42,
    } catch (replacement) {
        int => 7,
        baml.panics.Panic => -1,
    }
}

test "arm throw from catch_all_panics reaches later catch" {
    assert.equal(cc_arm_throw_catch_all_panics_then_catch(), 7)
}

// The declared closed contract also checks that the later clause consumes
// the new error effect produced by the first handler.
function cc_arm_throw_catch_all_panics_then_catch_all() -> int throws never {
    {
        throw "original"
    } catch_all_panics (original) {
        _ => throw 42,
    } catch_all (replacement) {
        int => 7,
        baml.panics.Panic => -1,
    }
}

test "arm throw from catch_all_panics reaches later catch_all" {
    assert.equal(cc_arm_throw_catch_all_panics_then_catch_all(), 7)
}

// The declared closed contract also checks that the later clause consumes
// the new error effect produced by the first handler.
function cc_arm_throw_catch_all_panics_then_catch_all_panics() -> int throws never {
    {
        throw "original"
    } catch_all_panics (original) {
        _ => throw 42,
    } catch_all_panics (replacement) {
        int => 7,
        baml.panics.Panic => -1,
    }
}

test "arm throw from catch_all_panics reaches later catch_all_panics" {
    assert.equal(cc_arm_throw_catch_all_panics_then_catch_all_panics(), 7)
}

class ChainedTimeout {
    ms: int,
}

function cc_timeout_source() -> string {
    throw ChainedTimeout { ms: 5 }
}

function cc_timeout_rethrow(events: string[]) -> string throws never {
    cc_timeout_source() catch (first, first_context) {
        ChainedTimeout => {
            events.push("timeout handler 1");
            throw first
        },
    } catch (second, second_context) {
        ChainedTimeout => {
            events.push("timeout handler 2");
            let before = first_line_for_timeout(second_context);
            if (second_context.cause != null) {
                "unexpected cause"
            } else {
                before
            }
        },
    }
}

function first_line_for_timeout(ctx: baml.errors.Context) -> string {
    let frames = ctx.stack_trace.frames;
    let origin = frames[frames.length() - 1];
    if (origin.function_name.includes("cc_timeout_source")) {
        "timeout error"
    } else {
        "lost original trace"
    }
}

function cc_timeout_log() -> string {
    let events: string[] = [];
    let result = cc_timeout_rethrow(events);
    events.join("|") + ";" + result
}

test "a chained handler rethrow runs both timeout handlers and preserves context" {
    assert.equal(cc_timeout_log(), "timeout handler 1|timeout handler 2;timeout error")
}

function cc_panic_in_arm() -> string {
    {
        throw "original"
    } catch (e) {
        string => {
            let xs = [1];
            xs[5].to_string()
        },
    } catch_all_panics (panic, context) {
        baml.panics.IndexOutOfBounds => {
            match (context.cause) {
                let cause: baml.errors.Context => cause.error.to_string(),
                _ => "missing cause",
            }
        },
        _ => "unexpected error",
    }
}

test "a panic raised by an earlier arm reaches a later boundary with its cause" {
    assert.equal(cc_panic_in_arm(), "original")
}

function cc_lambda_in_arm() -> string throws never {
    {
        throw "original"
    } catch (first) {
        string => {
            let fail = () -> string throws bool {
                throw true
            };
            fail()
        },
    } catch_all (second) {
        bool => "lambda recovered",
    }
}

test "a throwing lambda called by an arm reaches the next clause" {
    assert.equal(cc_lambda_in_arm(), "lambda recovered")
}

function cc_original_or_int(n: int) -> string {
    match (n) {
        0 => throw "original",
        _ => throw 1,
    }
}

function cc_sibling_arm_does_not_catch() -> string {
    cc_original_or_int(0) catch (e) {
        string => throw 42,
        int => "incorrect sibling",
    } catch (outer) {
        int => "later clause",
    }
}

test "an arm throw bypasses sibling arms and reaches a later clause" {
    assert.equal(cc_sibling_arm_does_not_catch(), "later clause")
}

function cc_successful_arm_stops() -> string {
    {
        throw "original"
    } catch (first) {
        string => "recovered",
    } catch_all (second) {
        _ => "incorrect later handler",
    }
}

test "a successful earlier arm skips later handlers" {
    assert.equal(cc_successful_arm_stops(), "recovered")
}

function cc_three_handlers() -> string throws never {
    {
        throw "original"
    } catch (first) {
        string => throw 42,
    } catch_all (second) {
        int => throw true,
    } catch_all (third) {
        bool => "third handler",
    }
}

test "three chained handlers catch each preceding handler's errors" {
    assert.equal(cc_three_handlers(), "third handler")
}

function cc_captured_first_binding() -> string {
    let remembered: (() -> string throws never)? = null;
    {
        throw "first"
    } catch (first) {
        string => {
            remembered
                = () -> string {
                    first
                };
            throw 42
        },
    } catch (second) {
        int => 0,
    };
    match (remembered) {
        let read: () -> string throws never => read(),
        _ => "missing captured binding",
    }
}

test "captured earlier clause bindings survive a later handler landing" {
    assert.equal(cc_captured_first_binding(), "first")
}

function cc_arm_defer_before_next_handler() -> string {
    let events: string[] = [];
    {
        throw "original"
    } catch (first) {
        string => {
            defer { events.push("cleanup"); }
            throw 42
        },
    } catch (second) {
        int => {
            events.push("next handler");
            0
        },
    };
    events.join("|")
}

test "a throwing arm runs its defer before the next catch handler" {
    assert.equal(cc_arm_defer_before_next_handler(), "cleanup|next handler")
}

function cc_or_pattern_source(n: int) -> string {
    match (n) {
        0 => throw "ordinary error",
        1 => cc_index_panic().to_string(),
        _ => baml.sys.panic("another panic"),
    }
}

function cc_named_panic_or_wildcard(n: int) -> string throws never {
    cc_or_pattern_source(n) catch (e) {
        baml.panics.IndexOutOfBounds | _ => "accepted",
    } catch (outer) {
        baml.panics.Panic => "propagated",
    }
}

function cc_wildcard_or_named_panic(n: int) -> string throws never {
    cc_or_pattern_source(n) catch_all (e) {
        _ | baml.panics.IndexOutOfBounds => "accepted",
    } catch (outer) {
        baml.panics.Panic => "propagated",
    }
}

test "catch Or-pattern wildcards skip panics without skipping named alternatives" {
    assert.equal(cc_named_panic_or_wildcard(0), "accepted");
    assert.equal(cc_named_panic_or_wildcard(1), "accepted");
    assert.equal(cc_named_panic_or_wildcard(2), "propagated");
    assert.equal(cc_wildcard_or_named_panic(0), "accepted");
    assert.equal(cc_wildcard_or_named_panic(1), "accepted");
    assert.equal(cc_wildcard_or_named_panic(2), "propagated")
}

function cc_boundary_or_wildcard(n: int) -> string throws never {
    cc_or_pattern_source(n) catch_all_panics (e) {
        baml.panics.IndexOutOfBounds | _ => "accepted",
    }
}

test "catch_all_panics Or-pattern wildcards still receive every panic" {
    assert.equal(cc_boundary_or_wildcard(0), "accepted");
    assert.equal(cc_boundary_or_wildcard(1), "accepted");
    assert.equal(cc_boundary_or_wildcard(2), "accepted")
}
