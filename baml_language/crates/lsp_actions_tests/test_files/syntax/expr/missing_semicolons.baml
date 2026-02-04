// Test cases for semicolon enforcement.
// In Rust-style syntax:
// - All statements and expressions require semicolons
// - EXCEPT: The last expression in a block (tail/return expression) does NOT need a semicolon
// - break/continue/return are expressions of type ! (never type) - semicolons optional

// ============================================================================
// VALID: These should NOT produce errors
// ============================================================================

// Tail expression without semicolon (return value)
function ValidReturn(x: int) -> int {
    let y = x + 1;
    y  // No semicolon - this is the return value
}

// Control flow as tail expression (no semicolon needed)
function ValidIfReturn(x: int) -> int {
    if (x > 0) {
        1
    } else {
        -1
    }  // No semicolon - if/else is the return value
}

// Nested blocks as tail expression
function ValidBlocks(x: int) -> int {
    {
        let a = 1;
        a
    }  // No semicolon - block is the return value
}

// Control flow expressions WITH semicolons (not tail)
function ValidControlFlowWithSemicolons(x: int) -> int {
    let result = 0;
    if (x > 0) {
        result = 1;
    };  // Semicolon required - not the tail expression
    while (false) {
        break;
    };  // Semicolon required - not the tail expression
    result
}

// break/continue/return - semicolons optional (type ! expressions)
function ValidBreakNoSemicolon() -> int {
    while (true) {
        break  // No semicolon needed - break is type !
    };
    0
}

function ValidContinueNoSemicolon() -> int {
    while (true) {
        continue  // No semicolon needed - continue is type !
    };
    0
}

function ValidReturnNoSemicolon(x: int) -> int {
    if (x == 0) {
        return 0  // No semicolon needed - return is type !
    };
    x
}

// break/continue/return with semicolons are also valid
function ValidBreakWithSemicolon() -> int {
    while (true) {
        break;  // Semicolon is valid
    };
    0
}

function ValidContinueWithSemicolon() -> int {
    while (true) {
        continue;  // Semicolon is valid
    };
    0
}

function ValidReturnWithSemicolon(x: int) -> int {
    if (x == 0) {
        return 0;  // Semicolon is valid
    };
    x
}

// ============================================================================
// INVALID: These SHOULD produce errors
// ============================================================================

// let statement missing semicolon (not the last expression)
function InvalidLet(x: int) -> int {
    let y = "hello"  // ERROR: needs semicolon
    x
}

// let with if expression missing semicolon
function InvalidLetWithIf(x: int) -> int {
    let y = if (x > 0) { 1 } else { 2 }  // ERROR: needs semicolon
    y
}

// Top-level let missing semicolon
let topLevel = 1  // ERROR: needs semicolon

// Control flow missing semicolon (not tail expression)
function InvalidIfNoSemicolon(x: int) -> int {
    let result = 0;
    if (x > 0) {
        result = 1;
    }  // ERROR: needs semicolon - not the tail expression
    result
}

function InvalidWhileNoSemicolon() -> int {
    while (false) {
        break;
    }  // ERROR: needs semicolon - not the tail expression
    0
}

// Assignment missing semicolon (not last)
function InvalidAssignment(x: int) -> int {
    let y = 0;
    y = x + 1  // ERROR: needs semicolon
    y
}

// Deeply nested test case with multiple corner cases
function DeeplyNested(x: int) -> int {
    let a = 0;
    if (x > 0) {
        // Nested while inside if - while needs semicolon
        while (a < 10) {
            a = a + 1;
            if (a == 5) {
                break  // break doesn't need semicolon (type !)
            };  // inner if needs semicolon
        };  // while needs semicolon

        // Nested if that's the tail of this branch
        if (a > 5) {
            100
        } else {
            50
        }  // This if/else IS the tail expression of this branch, no semicolon needed
    } else {
        // Another nested block
        {
            let b = x * 2;
            b  // tail expression of block
        }  // Block IS tail expression, no semicolon needed
    }  // This if/else IS the tail expression of the function, no semicolon needed
}

// Missing return expression - function expects int but body ends with a statement
function MissingReturnExpression() -> int {
    let x = 42;  // This is a statement, not a return expression
    // ERROR: missing return expression
}

//----
//- diagnostics
// Error: Statement must end with a semicolon.
//      ╭─[ expr_missing_semicolons.baml:104:2 ]
//      │
//  104 │ ╭─▶ }
//      ┆ ┆   
//  107 │ ├─▶ let topLevel = 1  // ERROR: needs semicolon
//      │ │                                                 
//      │ ╰───────────────────────────────────────────────── Statement must end with a semicolon.
//      │     
//      │     Note: Error code: E0028
// ─────╯
// Error: Statement must end with a semicolon.
//      ╭─[ expr_missing_semicolons.baml:102:5 ]
//      │
//  102 │     let y = if (x > 0) { 1 } else { 2 }  // ERROR: needs semicolon
//      │     ─────────────────┬─────────────────  
//      │                      ╰─────────────────── Statement must end with a semicolon.
//      │ 
//      │ Note: Error code: E0028
// ─────╯
// Error: Statement must end with a semicolon.
//     ╭─[ expr_missing_semicolons.baml:96:5 ]
//     │
//  96 │     let y = "hello"  // ERROR: needs semicolon
//     │     ───────┬───────  
//     │            ╰───────── Statement must end with a semicolon.
//     │ 
//     │ Note: Error code: E0028
// ────╯
// Error: Missing return expression. Function expects `int` but body has no final expression.
//      ╭─[ expr_missing_semicolons.baml:160:42 ]
//      │
//  160 │ ╭─▶ function MissingReturnExpression() -> int {
//      ┆ ┆   
//  163 │ ├─▶ }
//      │ │      
//      │ ╰────── Missing return expression. Function expects `int` but body has no final expression.
//      │     
//      │     Note: Error code: E0029
// ─────╯
