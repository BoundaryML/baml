// Test: Catch/throw MIR lowering
// Each function exercises a specific catch/throw pattern.

// Helper functions used as call targets
function MayFail(x: int) -> string {
    match (x) {
        0 => throw "string error",
        _ => "ok"
    }
}

function MayFailIntOrString(x: int) -> string {
    match (x) {
        0 => throw "string error",
        1 => throw 1,
        _ => "ok"
    }
}

function AlsoMayFail(x: int) -> string {
    match (x) {
        0 => throw "secondary error",
        _ => "also ok"
    }
}

// ============================================================================
// SINGLE CATCH CLAUSE
// ============================================================================

// Basic: call with a single catch clause containing one wildcard arm
function SingleCatchWildcard(x: int) -> string {
    MayFail(x) catch (e) {
        _ => "caught"
    }
}

// Single catch with typed arm
function SingleCatchTyped(x: int) -> string {
    MayFail(x) catch (e) {
        _: string => "caught string"
    }
}

// ============================================================================
// CATCH WITH FALLBACK
// ============================================================================

// Additional catch clause for fallback behavior
function CatchWithFallback(x: int) -> string {
    MayFailIntOrString(x) catch (e) {
        _: string => "caught string"
    } catch (e2) {
        _: int => "caught int fallback"
    }
}

// ============================================================================
// CATCH WITH TYPED ARM
// ============================================================================

// Catch with typed arm pattern
function CatchWithTypedArm(x: int) -> string {
    MayFail(x) catch (e) {
        _: string => "caught string"
    }
}

// ============================================================================
// MULTIPLE CATCH ARMS
// ============================================================================

// Multiple typed arms in a single catch
function MultipleCatchArms(x: int) -> string {
    MayFailIntOrString(x) catch (e) {
        _: string => "caught string",
        _: int => "caught int"
    }
}

// ============================================================================
// NESTED CATCH
// ============================================================================

// Nested catch: inner catch wraps one call, outer catch wraps a different call
function NestedCatch(x: int) -> string {
    let inner = MayFail(x) catch (e) {
        _ => "inner caught"
    };
    AlsoMayFail(x) catch (e) {
        _ => inner
    }
}

// ============================================================================
// THROW EXPRESSION
// ============================================================================

// Throw re-raises the caught error
function CatchAndRethrow(x: int) -> string {
    MayFail(x) catch (e) {
        _ => throw e
    }
}

// ============================================================================
// THROW STATEMENT
// ============================================================================

// Throw as a statement
function ThrowStatement(x: int) -> string {
    throw "error"
    "unreachable"
}

// ============================================================================
// CHAINED CATCH CLAUSES
// ============================================================================

// Multiple catch clauses on the same expression
function ChainedCatchClauses(x: int) -> string {
    MayFailIntOrString(x) catch (e) {
        _: string => "first handler"
    } catch (e2) {
        _: int => "second handler"
    }
}
