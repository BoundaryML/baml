// ============================================================================
// BUILTIN FUNCTION ERROR TESTS
// ============================================================================
//
// Verify that builtin functions throw their declared error types and that
// catch expressions can handle them.
//
// ============================================================================

// ============================================================================
// Array.at — throws IndexError
// ============================================================================

// Catch an out-of-bounds access with a default value
function ArrayAtCatch() -> int {
    let arr = [10, 20, 30];
    baml.Array.at(arr, 99) catch {
        e: IndexError => -1
    }
}

// Catch-all binding on array access
function ArrayAtCatchAll() -> int {
    let arr = [1, 2, 3];
    baml.Array.at(arr, 5) catch_all {
        e => 0
    }
}

// Successful access — no error thrown, catch is not entered
function ArrayAtSuccess() -> int {
    let arr = [10, 20, 30];
    baml.Array.at(arr, 1) catch {
        e: IndexError => -1
    }
}

// Typed binding with IndexError on a string array
function ArrayAtTypedBinding() -> string {
    let arr = ["a", "b", "c"];
    baml.Array.at(arr, 10) catch_all {
        e: IndexError => "not found"
    }
}

// ============================================================================
// deep_copy — throws ValueError
// ============================================================================

// Catch deep_copy (normally succeeds for simple values)
function DeepCopyCatch() -> int {
    baml.deep_copy(42) catch {
        e: ValueError => -1
    }
}

// ============================================================================
// NESTED CATCH — catch around a catch
// ============================================================================

// Outer catch handles first error, inner body may also throw
function NestedBuiltinCatch() -> int {
    let arr = [1, 2, 3];
    baml.Array.at(arr, 100) catch {
        e: IndexError => {
            // Handle the error — try a valid access in the handler
            baml.Array.at(arr, 0) catch {
                f: IndexError => -1
            }
        }
    }
}

// ============================================================================
// WILDCARD CATCH — use _ to discard the error
// ============================================================================

function ArrayAtWildcard() -> int {
    let arr = [1, 2, 3];
    baml.Array.at(arr, 999) catch_all {
        _ => 42
    }
}
