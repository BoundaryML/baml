//! Tests that show the effect of MIR-level optimizations by comparing
//! bytecode output at `OptLevel::One` (no constant folding) vs `OptLevel::Two` (full optimization).
//!
//! Each test compiles the same BAML source twice and snapshots both versions
//! so the optimization effect is visible at a glance in the source file.

use baml_tests::engine::{OptLevel, compile_source_with_opt, display_user_functions};

/// Compile source at OptLevel::One (no constant folding) and return textual bytecode.
fn unoptimized(source: &str) -> String {
    let program = compile_source_with_opt(source, OptLevel::One);
    display_user_functions(&program)
}

/// Compile source at OptLevel::Two (with constant folding) and return textual bytecode.
fn optimized(source: &str) -> String {
    let program = compile_source_with_opt(source, OptLevel::Two);
    display_user_functions(&program)
}

// ============================================================================
// Constant Folding — Binary Ops
// ============================================================================

#[test]
fn constant_fold_int_addition() {
    let source = r#"
        function main() -> int {
            2 + 3
        }
    "#;
    insta::assert_snapshot!(unoptimized(source), @"
    function main() -> int {
        load_const 2
        load_const 3
        add_int
        return
    }
    ");
    insta::assert_snapshot!(optimized(source), @r"
    function main() -> int {
        load_const 5
        return
    }
    ");
}

#[test]
fn constant_fold_int_arithmetic_chain() {
    let source = r#"
        function main() -> int {
            (10 * 3) + (100 - 50) - 1
        }
    "#;
    insta::assert_snapshot!(unoptimized(source), @"
    function main() -> int {
        load_const 10
        load_const 3
        mul_int
        load_const 100
        load_const 50
        sub_int
        add_int
        load_const 1
        sub_int
        return
    }
    ");
    insta::assert_snapshot!(optimized(source), @r"
    function main() -> int {
        load_const 79
        return
    }
    ");
}

#[test]
fn constant_fold_float_arithmetic() {
    let source = r#"
        function main() -> float {
            1.5 + 2.5
        }
    "#;
    insta::assert_snapshot!(unoptimized(source), @"
    function main() -> float {
        load_const 1.5
        load_const 2.5
        add_float
        return
    }
    ");
    insta::assert_snapshot!(optimized(source), @r"
    function main() -> float {
        load_const 4.0
        return
    }
    ");
}

#[test]
fn constant_fold_string_concat() {
    let source = r#"
        function main() -> string {
            "hello" + " " + "world"
        }
    "#;
    insta::assert_snapshot!(unoptimized(source), @r#"
    function main() -> string {
        load_const "hello"
        load_const " "
        bin_op +
        load_const "world"
        bin_op +
        return
    }
    "#);
    insta::assert_snapshot!(optimized(source), @r#"
    function main() -> string {
        load_const "hello world"
        return
    }
    "#);
}

#[test]
fn constant_fold_int_comparison() {
    let source = r#"
        function main() -> bool {
            10 > 5
        }
    "#;
    insta::assert_snapshot!(unoptimized(source), @"
    function main() -> bool {
        load_const 10
        load_const 5
        cmp_int_op >
        return
    }
    ");
    insta::assert_snapshot!(optimized(source), @r"
    function main() -> bool {
        load_const true
        return
    }
    ");
}

#[test]
fn constant_fold_mixed_not_foldable() {
    // x is a runtime variable — the addition cannot be folded.
    // Both outputs should be identical.
    let source = r#"
        function main(x: int) -> int {
            x + 1
        }
    "#;
    insta::assert_snapshot!(unoptimized(source), @"
    function main(x: int) -> int {
        load_var x
        load_const 1
        add_int
        return
    }
    ");
    insta::assert_snapshot!(optimized(source), @"
    function main(x: int) -> int {
        load_var x
        load_const 1
        add_int
        return
    }
    ");
}

// ============================================================================
// Constant Folding — Unary Ops
// ============================================================================

#[test]
fn constant_fold_unary_negation() {
    let source = r#"
        function main() -> int {
            -42
        }
    "#;
    insta::assert_snapshot!(unoptimized(source), @r"
    function main() -> int {
        load_const 42
        unary_op -
        return
    }
    ");
    insta::assert_snapshot!(optimized(source), @r"
    function main() -> int {
        load_const -42
        return
    }
    ");
}

#[test]
fn constant_fold_unary_not() {
    let source = r#"
        function main() -> bool {
            !true
        }
    "#;
    insta::assert_snapshot!(unoptimized(source), @r"
    function main() -> bool {
        load_const true
        unary_op !
        return
    }
    ");
    insta::assert_snapshot!(optimized(source), @r"
    function main() -> bool {
        load_const false
        return
    }
    ");
}

// ============================================================================
// Combined: Constant Folding + Struct Construction
// ============================================================================

#[test]
fn combined_constant_fold_and_struct() {
    // Constant folding (2 + 3 → 5) inside struct field initializer.
    let source = r#"
        class Result {
            value: int
            label: string
        }

        function main() -> Result {
            Result { value: 2 + 3, label: "sum" }
        }
    "#;
    insta::assert_snapshot!(unoptimized(source), @r#"
    function main() -> <unresolved type #55062759600785> {
        load_const 2
        load_const 3
        add_int
        load_const "sum"
        init_instance user.Result .value, .label
        return
    }
    "#);
    insta::assert_snapshot!(optimized(source), @r#"
    function main() -> <unresolved type #55062759600785> {
        load_const 5
        load_const "sum"
        init_instance user.Result .value, .label
        return
    }
    "#);
}
