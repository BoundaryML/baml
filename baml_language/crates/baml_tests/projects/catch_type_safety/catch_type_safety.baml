// ============================================================================
// CATCH TYPE SAFETY AND EXHAUSTIVENESS TESTS
// ============================================================================
//
// These tests verify that:
// 1. Pattern bindings work in catch arms
// 2. Thrown types are properly tracked and propagated
// 3. Catch expressions are checked for exhaustiveness
// 4. Unreachable catch arms are detected
//
// ============================================================================

// ============================================================================
// PATTERN BINDING TESTS
// Verify that catch arm bindings are visible in arm bodies
// ============================================================================

// Simple binding: caught value accessible in arm body
function CatchBindingSimple() -> int {
    throw 42 catch_all {
        e => e  // e should have type int
    }
}

// Typed binding: pattern with type annotation
function CatchBindingTyped() -> string {
    throw "hello" catch_all {
        e: string => e
    }
}

// Multiple catch arms with bindings
function CatchMultipleBindings() -> int {
    throw 42 catch {
        e: int => e * 2
        f => f  // catch-all binding
    }
}

// ============================================================================
// EXHAUSTIVENESS TESTS - catch_all (must be exhaustive)
// ============================================================================

// Exhaustive: catch-all pattern covers all thrown types
function CatchAllExhaustiveWildcard() -> int {
    throw 42 catch_all {
        _ => 0  // Wildcard covers everything
    }
}

// Exhaustive: named binding covers all thrown types
function CatchAllExhaustiveNamed() -> int {
    throw 42 catch_all {
        e => e  // Named binding covers everything
    }
}

// Exhaustive: typed pattern covers thrown type
function CatchAllExhaustiveTyped() -> int {
    throw 42 catch_all {
        e: int => e  // int pattern covers thrown int
    }
}

// Exhaustive: union of typed patterns covers thrown union type
class ErrorA { code int }
class ErrorB { message string }

function CatchAllExhaustiveUnion() -> string {
    throw (ErrorA { code: 1 }) catch_all {
        e: ErrorA => "A",
        e: ErrorB => "B"
    }
}

// ============================================================================
// EXHAUSTIVENESS TESTS - catch (always exhaustive via desugared rethrow)
// ============================================================================

// Exhaustive: catch desugars to catch_all with rethrow arm
function CatchAlwaysExhaustive() -> int {
    throw 42 catch {
        e: int => e  // Missing cases rethrow automatically
    }
}

// Exhaustive: partial handling with automatic rethrow
function CatchPartialHandling() -> string {
    throw (ErrorA { code: 1 }) catch {
        e: ErrorA => "handled A"
        // ErrorB automatically rethrown
    }
}

// ============================================================================
// THROWN TYPE PROPAGATION TESTS
// Verify that thrown types propagate through expressions
// ============================================================================

// Thrown type propagates through binary operations
function ThrownTypePropagation() -> int {
    let x = throw 1 catch_all { e => e };
    let y = throw 2 catch_all { e => e };
    x + y  // Both thrown types should be tracked if rethrown
}

// Nested catch expressions
function NestedCatch() -> int {
    throw (throw 42 catch_all { e => e }) catch_all {
        e => e
    }
}

// ============================================================================
// RE-THROW TESTS
// Verify that catch arms can re-throw
// ============================================================================

// Re-throw in catch arm propagates the thrown type
function CatchRethrow() -> int {
    throw 42 catch {
        e: int => throw e  // Re-throws the caught value
    }
}

// Conditional re-throw using if-else
function CatchConditionalRethrow(x: bool) -> int {
    throw 42 catch {
        e: int => {
            if x {
                e
            } else {
                throw e
            }
        }
    }
}
