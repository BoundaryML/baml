// ============================================================================
// CATCH TYPE SAFETY ERROR TESTS
// ============================================================================
//
// These tests should produce compile-time errors:
// 1. Non-exhaustive catch_all blocks
// 2. Unreachable catch arms
// 3. Type mismatches in catch arm bodies
//
// ============================================================================

class ErrorA { code int }
class ErrorB { message string }

// ============================================================================
// NON-EXHAUSTIVE CATCH_ALL TESTS
// These should produce NonExhaustiveCatch errors
// ============================================================================

// ERROR: Non-exhaustive - int thrown but string pattern only
function Error_NonExhaustiveTypeMismatch() -> int {
    throw 42 catch_all {
        e: string => 0  // Error: thrown type int not covered
    }
}

// ERROR: Non-exhaustive - union type missing variant
function Error_NonExhaustiveUnion() -> string {
    throw (ErrorA { code: 1 }) catch_all {
        e: ErrorA => "A"  // Error: ErrorB not covered
    }
}

// ============================================================================
// UNREACHABLE CATCH ARM TESTS
// These should produce UnreachableCatchArm errors
// ============================================================================

// ERROR: Unreachable arm after catch-all
function Error_UnreachableAfterCatchAll() -> int {
    throw 42 catch_all {
        e => e,
        f: int => f  // Error: unreachable
    }
}

// ERROR: Multiple unreachable arms
function Error_MultipleUnreachable() -> int {
    throw 42 catch_all {
        _ => 0,
        a => a,      // Error: unreachable
        b: int => b   // Error: unreachable
    }
}

// ============================================================================
// TYPE MISMATCH TESTS
// These should produce TypeMismatch errors
// ============================================================================

// ERROR: Type mismatch in catch arm body
function Error_CatchArmTypeMismatch() -> string {
    throw 42 catch_all {
        e: int => e  // Error: returns int but function expects string
    }
}

// ERROR: Type mismatch in conditional arm
function Error_PartialArmTypeMismatch(x: bool) -> string {
    throw 42 catch_all {
        e: int => {
            if x {
                "ok"
            } else {
                e  // Error: else branch returns int
            }
        }
    }
}
