//! Unified tests for match optimizations: jump tables, binary search, density thresholds,
//! if-else chains, range limits, large jump table tests, union patterns aggregating to 4+ arms,
//! negative jump tables, spanning zero tests, binary search with negative values, and large
//! binary search tree.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ============================================================================
// Jump Table Tests (4+ dense arms trigger jump table optimization)
// ============================================================================

#[tokio::test]
async fn match_jump_table_first_arm() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 0;
            match (x) {
                0 => 100,
                1 => 101,
                2 => 102,
                3 => 103,
                _ => 999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 0
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1: 3
        load_const 103
        jump L5

      L2: 2
        load_const 102
        jump L5

      L3: 1
        load_const 101
        jump L5

      L4: 0
        load_const 100

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

#[tokio::test]
async fn match_jump_table_middle_arm() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 2;
            match (x) {
                0 => 100,
                1 => 101,
                2 => 102,
                3 => 103,
                _ => 999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 2
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1: 3
        load_const 103
        jump L5

      L2: 2
        load_const 102
        jump L5

      L3: 1
        load_const 101
        jump L5

      L4: 0
        load_const 100

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(102)));
}

#[tokio::test]
async fn match_jump_table_last_arm() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 3;
            match (x) {
                0 => 100,
                1 => 101,
                2 => 102,
                3 => 103,
                _ => 999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 3
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1: 3
        load_const 103
        jump L5

      L2: 2
        load_const 102
        jump L5

      L3: 1
        load_const 101
        jump L5

      L4: 0
        load_const 100

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(103)));
}

#[tokio::test]
async fn match_jump_table_fallback() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 10;
            match (x) {
                0 => 100,
                1 => 101,
                2 => 102,
                3 => 103,
                _ => 999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 10
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1: 3
        load_const 103
        jump L5

      L2: 2
        load_const 102
        jump L5

      L3: 1
        load_const 101
        jump L5

      L4: 0
        load_const 100

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(999)));
}

#[tokio::test]
async fn match_jump_table_negative_fallback() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = -1;
            match (x) {
                0 => 100,
                1 => 101,
                2 => 102,
                3 => 103,
                _ => 999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 1
        unary_op -
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1: 3
        load_const 103
        jump L5

      L2: 2
        load_const 102
        jump L5

      L3: 1
        load_const 101
        jump L5

      L4: 0
        load_const 100

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(999)));
}

#[tokio::test]
async fn match_jump_table_with_holes_miss() {
    // 4 values in range of 7: density ~57% (above 50% threshold)
    // Hole at value 1 should fall through to default
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 1;
            match (x) {
                0 => 100,
                2 => 102,
                4 => 104,
                6 => 106,
                _ => 999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 1
        jump_table [L4, _, L3, _, L2, _, L1], default L0

      L0:
        load_const 999
        jump L5

      L1: 6
        load_const 106
        jump L5

      L2: 4
        load_const 104
        jump L5

      L3: 2
        load_const 102
        jump L5

      L4: 0
        load_const 100

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(999)));
}

#[tokio::test]
async fn match_jump_table_with_holes_hit() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 4;
            match (x) {
                0 => 100,
                2 => 102,
                4 => 104,
                6 => 106,
                _ => 999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 4
        jump_table [L4, _, L3, _, L2, _, L1], default L0

      L0:
        load_const 999
        jump L5

      L1: 6
        load_const 106
        jump L5

      L2: 4
        load_const 104
        jump L5

      L3: 2
        load_const 102
        jump L5

      L4: 0
        load_const 100

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(104)));
}

#[tokio::test]
async fn match_jump_table_offset_values() {
    // Jump table with non-zero base offset
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 11;
            match (x) {
                10 => 110,
                11 => 111,
                12 => 112,
                13 => 113,
                _ => 999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 11
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1: 13
        load_const 113
        jump L5

      L2: 12
        load_const 112
        jump L5

      L3: 11
        load_const 111
        jump L5

      L4: 10
        load_const 110

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(111)));
}

#[tokio::test]
async fn match_jump_table_large() {
    // 8 consecutive values - should still use jump table
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 5;
            match (x) {
                0 => 1000,
                1 => 1001,
                2 => 1002,
                3 => 1003,
                4 => 1004,
                5 => 1005,
                6 => 1006,
                7 => 1007,
                _ => 9999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 5
        jump_table [L8, L7, L6, L5, L4, L3, L2, L1], default L0

      L0:
        load_const 9999
        jump L9

      L1: 7
        load_const 1007
        jump L9

      L2: 6
        load_const 1006
        jump L9

      L3: 5
        load_const 1005
        jump L9

      L4: 4
        load_const 1004
        jump L9

      L5: 3
        load_const 1003
        jump L9

      L6: 2
        load_const 1002
        jump L9

      L7: 1
        load_const 1001
        jump L9

      L8: 0
        load_const 1000

      L9:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1005)));
}

/// Tests that a match with 4 dense consecutive integer arms uses a jump table
/// with a function parameter (non-constant scrutinee).
#[tokio::test]
async fn match_jump_table_dense_four_arms_param() {
    let output = baml_test! {
        baml: r#"
            function classify(x: int) -> int {
                match (x) {
                    0 => 100,
                    1 => 101,
                    2 => 102,
                    3 => 103,
                    _ => 999
                }
            }
        "#,
        entry: "classify",
        args: { "x" => BexExternalValue::Int(2) },
    };

    insta::assert_snapshot!(output.bytecode, @"
    function classify(x: int) -> int {
        load_var x
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1: 3
        load_const 103
        jump L5

      L2: 2
        load_const 102
        jump L5

      L3: 1
        load_const 101
        jump L5

      L4: 0
        load_const 100

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(102)));
}

// ============================================================================
// Binary Search Tests (4+ sparse arms trigger binary search optimization)
// ============================================================================

#[tokio::test]
async fn match_binary_search_first_arm() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 0;
            match (x) {
                0 => 100,
                30 => 130,
                60 => 160,
                99 => 199,
                _ => 999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 0
        dense_tag [0, 30, 60, 99]
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1: 99
        load_const 199
        jump L5

      L2: 60
        load_const 160
        jump L5

      L3: 30
        load_const 130
        jump L5

      L4: 0
        load_const 100

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

#[tokio::test]
async fn match_binary_search_middle_arm() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 60;
            match (x) {
                0 => 100,
                30 => 130,
                60 => 160,
                99 => 199,
                _ => 999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 60
        dense_tag [0, 30, 60, 99]
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1: 99
        load_const 199
        jump L5

      L2: 60
        load_const 160
        jump L5

      L3: 30
        load_const 130
        jump L5

      L4: 0
        load_const 100

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(160)));
}

#[tokio::test]
async fn match_binary_search_last_arm() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 99;
            match (x) {
                0 => 100,
                30 => 130,
                60 => 160,
                99 => 199,
                _ => 999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 99
        dense_tag [0, 30, 60, 99]
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1: 99
        load_const 199
        jump L5

      L2: 60
        load_const 160
        jump L5

      L3: 30
        load_const 130
        jump L5

      L4: 0
        load_const 100

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(199)));
}

#[tokio::test]
async fn match_binary_search_fallback() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 50;
            match (x) {
                0 => 100,
                30 => 130,
                60 => 160,
                99 => 199,
                _ => 999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 50
        dense_tag [0, 30, 60, 99]
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1: 99
        load_const 199
        jump L5

      L2: 60
        load_const 160
        jump L5

      L3: 30
        load_const 130
        jump L5

      L4: 0
        load_const 100

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(999)));
}

#[tokio::test]
async fn match_binary_search_very_sparse() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 500;
            match (x) {
                0 => 1000,
                100 => 1100,
                200 => 1200,
                300 => 1300,
                400 => 1400,
                500 => 1500,
                _ => 9999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 500
        dense_tag [0, 100, 200, 300, 400, 500]
        jump_table [L6, L5, L4, L3, L2, L1], default L0

      L0:
        load_const 9999
        jump L7

      L1: 500
        load_const 1500
        jump L7

      L2: 400
        load_const 1400
        jump L7

      L3: 300
        load_const 1300
        jump L7

      L4: 200
        load_const 1200
        jump L7

      L5: 100
        load_const 1100
        jump L7

      L6: 0
        load_const 1000

      L7:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1500)));
}

#[tokio::test]
async fn match_binary_search_large_values() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 750;
            match (x) {
                250 => 1250,
                500 => 1500,
                750 => 1750,
                1000 => 2000,
                1250 => 2250,
                _ => 9999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 750
        dense_tag [250, 500, 750, 1000, 1250]
        jump_table [L5, L4, L3, L2, L1], default L0

      L0:
        load_const 9999
        jump L6

      L1: 1250
        load_const 2250
        jump L6

      L2: 1000
        load_const 2000
        jump L6

      L3: 750
        load_const 1750
        jump L6

      L4: 500
        load_const 1500
        jump L6

      L5: 250
        load_const 1250

      L6:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1750)));
}

/// Binary search with function parameter (non-constant scrutinee).
#[tokio::test]
async fn match_binary_search_sparse_four_arms_param() {
    let output = baml_test! {
        baml: r#"
            function classify(x: int) -> int {
                match (x) {
                    0 => 100,
                    30 => 130,
                    60 => 160,
                    99 => 199,
                    _ => 999
                }
            }
        "#,
        entry: "classify",
        args: { "x" => BexExternalValue::Int(30) },
    };

    insta::assert_snapshot!(output.bytecode, @"
    function classify(x: int) -> int {
        load_var x
        dense_tag [0, 30, 60, 99]
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1: 99
        load_const 199
        jump L5

      L2: 60
        load_const 160
        jump L5

      L3: 30
        load_const 130
        jump L5

      L4: 0
        load_const 100

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(130)));
}

// ============================================================================
// If-Else Chain Tests (< 4 arms)
// ============================================================================

/// Tests that a match with fewer than 4 integer arms uses if-else chain.
#[tokio::test]
async fn match_if_else_chain_three_arms() {
    let output = baml_test! {
        baml: r#"
            function classify(x: int) -> int {
                match (x) {
                    0 => 100,
                    1 => 101,
                    _ => 999
                }
            }
        "#,
        entry: "classify",
        args: { "x" => BexExternalValue::Int(0) },
    };

    insta::assert_snapshot!(output.bytecode, @"
    function classify(x: int) -> int {
        load_var x
        load_const 0
        cmp_int_op ==
        pop_jump_if_false L0
        jump L3

      L0:
        load_var x
        load_const 1
        cmp_int_op ==
        pop_jump_if_false L1
        jump L2

      L1:
        load_const 999
        jump L4

      L2:
        load_const 101
        jump L4

      L3:
        load_const 100

      L4:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

// ============================================================================
// Density Threshold Tests (boundary at 50%)
// ============================================================================

#[tokio::test]
async fn match_density_exactly_50_percent() {
    // 4 arms in range of 8: 0, 2, 4, 6 = 50% density (should use jump table)
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 4;
            match (x) {
                0 => 100,
                2 => 102,
                4 => 104,
                6 => 106,
                _ => 999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 4
        jump_table [L4, _, L3, _, L2, _, L1], default L0

      L0:
        load_const 999
        jump L5

      L1: 6
        load_const 106
        jump L5

      L2: 4
        load_const 104
        jump L5

      L3: 2
        load_const 102
        jump L5

      L4: 0
        load_const 100

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(104)));
}

#[tokio::test]
async fn match_density_below_50_percent() {
    // 4 arms in range of 10: 0, 3, 6, 9 = 40% density (should use binary search)
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 6;
            match (x) {
                0 => 100,
                3 => 103,
                6 => 106,
                9 => 109,
                _ => 999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 6
        dense_tag [0, 3, 6, 9]
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1: 9
        load_const 109
        jump L5

      L2: 6
        load_const 106
        jump L5

      L3: 3
        load_const 103
        jump L5

      L4: 0
        load_const 100

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(106)));
}

#[tokio::test]
async fn match_density_above_50_percent() {
    // 5 arms in range of 6: 0, 1, 2, 4, 5 = 83% density (should use jump table)
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 4;
            match (x) {
                0 => 100,
                1 => 101,
                2 => 102,
                4 => 104,
                5 => 105,
                _ => 999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 4
        jump_table [L5, L4, L3, _, L2, L1], default L0

      L0:
        load_const 999
        jump L6

      L1: 5
        load_const 105
        jump L6

      L2: 4
        load_const 104
        jump L6

      L3: 2
        load_const 102
        jump L6

      L4: 1
        load_const 101
        jump L6

      L5: 0
        load_const 100

      L6:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(104)));
}

// ============================================================================
// Large Range Integer Values
// ============================================================================

#[tokio::test]
async fn match_large_range_dense() {
    // Dense values starting from offset (100-103)
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 102;
            match (x) {
                100 => 1000,
                101 => 1001,
                102 => 1002,
                103 => 1003,
                _ => 9999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 102
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 9999
        jump L5

      L1: 103
        load_const 1003
        jump L5

      L2: 102
        load_const 1002
        jump L5

      L3: 101
        load_const 1001
        jump L5

      L4: 100
        load_const 1000

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1002)));
}

#[tokio::test]
async fn match_large_range_sparse() {
    // Sparse values with large gaps
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 1000;
            match (x) {
                0 => 1,
                500 => 2,
                1000 => 3,
                1500 => 4,
                _ => 9999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 1000
        dense_tag [0, 500, 1000, 1500]
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 9999
        jump L5

      L1: 1500
        load_const 4
        jump L5

      L2: 1000
        load_const 3
        jump L5

      L3: 500
        load_const 2
        jump L5

      L4: 0
        load_const 1

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn match_zero_in_range() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 0;
            match (x) {
                0 => 100,
                1 => 101,
                2 => 102,
                3 => 103,
                _ => 999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 0
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1: 3
        load_const 103
        jump L5

      L2: 2
        load_const 102
        jump L5

      L3: 1
        load_const 101
        jump L5

      L4: 0
        load_const 100

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

// ============================================================================
// Negative Integer Jump Table Tests
// ============================================================================

#[tokio::test]
async fn match_negative_jump_table() {
    // Dense negative range should use jump table: -3, -2, -1, 0
    let output = baml_test!(
        r#"
        function classify(x: int) -> string {
            match (x) {
                -3 => "neg three",
                -2 => "neg two",
                -1 => "neg one",
                0 => "zero",
                _ => "other"
            }
        }
        function main() -> string {
            classify(-2)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(x: int) -> string {
        load_var x
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const "other"
        jump L5

      L1: 0
        load_const "zero"
        jump L5

      L2: -1
        load_const "neg one"
        jump L5

      L3: -2
        load_const "neg two"
        jump L5

      L4: -3
        load_const "neg three"

      L5:
        return
    }

    function main() -> string {
        load_const 2
        unary_op -
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("neg two".to_string()))
    );
}

#[tokio::test]
async fn match_negative_jump_table_fallback() {
    let output = baml_test!(
        r#"
        function classify(x: int) -> string {
            match (x) {
                -3 => "neg three",
                -2 => "neg two",
                -1 => "neg one",
                0 => "zero",
                _ => "other"
            }
        }
        function main() -> string {
            classify(5)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(x: int) -> string {
        load_var x
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const "other"
        jump L5

      L1: 0
        load_const "zero"
        jump L5

      L2: -1
        load_const "neg one"
        jump L5

      L3: -2
        load_const "neg two"
        jump L5

      L4: -3
        load_const "neg three"

      L5:
        return
    }

    function main() -> string {
        load_const 5
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("other".to_string()))
    );
}

// ============================================================================
// Mixed Positive/Negative Spanning Zero Tests
// ============================================================================

#[tokio::test]
async fn match_spanning_zero_jump_table() {
    let output = baml_test!(
        r#"
        function classify(x: int) -> string {
            match (x) {
                -2 => "neg two",
                -1 => "neg one",
                0 => "zero",
                1 => "one",
                2 => "two",
                _ => "other"
            }
        }
        function main() -> string {
            classify(1)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(x: int) -> string {
        load_var x
        jump_table [L5, L4, L3, L2, L1], default L0

      L0:
        load_const "other"
        jump L6

      L1: 2
        load_const "two"
        jump L6

      L2: 1
        load_const "one"
        jump L6

      L3: 0
        load_const "zero"
        jump L6

      L4: -1
        load_const "neg one"
        jump L6

      L5: -2
        load_const "neg two"

      L6:
        return
    }

    function main() -> string {
        load_const 1
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("one".to_string()))
    );
}

#[tokio::test]
async fn match_spanning_zero_negative_hit() {
    let output = baml_test!(
        r#"
        function classify(x: int) -> string {
            match (x) {
                -2 => "neg two",
                -1 => "neg one",
                0 => "zero",
                1 => "one",
                2 => "two",
                _ => "other"
            }
        }
        function main() -> string {
            classify(-1)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(x: int) -> string {
        load_var x
        jump_table [L5, L4, L3, L2, L1], default L0

      L0:
        load_const "other"
        jump L6

      L1: 2
        load_const "two"
        jump L6

      L2: 1
        load_const "one"
        jump L6

      L3: 0
        load_const "zero"
        jump L6

      L4: -1
        load_const "neg one"
        jump L6

      L5: -2
        load_const "neg two"

      L6:
        return
    }

    function main() -> string {
        load_const 1
        unary_op -
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("neg one".to_string()))
    );
}

// ============================================================================
// Binary Search with Negative Values
// ============================================================================

#[tokio::test]
async fn match_binary_search_negative_sparse() {
    let output = baml_test!(
        r#"
        function classify(x: int) -> string {
            match (x) {
                -100 => "a",
                -50 => "b",
                -10 => "c",
                -1 => "d",
                _ => "other"
            }
        }
        function main() -> string {
            classify(-50)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(x: int) -> string {
        load_var x
        dense_tag [-100, -50, -10, -1]
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const "other"
        jump L5

      L1: -1
        load_const "d"
        jump L5

      L2: -10
        load_const "c"
        jump L5

      L3: -50
        load_const "b"
        jump L5

      L4: -100
        load_const "a"

      L5:
        return
    }

    function main() -> string {
        load_const 50
        unary_op -
        call user.classify
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::String("b".to_string())));
}

#[tokio::test]
async fn match_binary_search_spanning_zero_sparse() {
    let output = baml_test!(
        r#"
        function classify(x: int) -> string {
            match (x) {
                -100 => "neg hundred",
                -1 => "neg one",
                1 => "one",
                100 => "hundred",
                _ => "other"
            }
        }
        function main() -> string {
            classify(1)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(x: int) -> string {
        load_var x
        dense_tag [-100, -1, 1, 100]
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const "other"
        jump L5

      L1: 100
        load_const "hundred"
        jump L5

      L2: 1
        load_const "one"
        jump L5

      L3: -1
        load_const "neg one"
        jump L5

      L4: -100
        load_const "neg hundred"

      L5:
        return
    }

    function main() -> string {
        load_const 1
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("one".to_string()))
    );
}

// ============================================================================
// Union Patterns Aggregating to 4+ Arms Tests
// ============================================================================

/// Union patterns that aggregate to 4+ total values should use jump table.
#[tokio::test]
async fn match_union_aggregated_jump_table() {
    let output = baml_test! {
        baml: r#"
            function classify(x: int) -> string {
                match (x) {
                    0 | 1 => "a",
                    2 | 3 => "b",
                    4 | 5 => "c",
                    6 | 7 => "d",
                    _ => "other"
                }
            }
        "#,
        entry: "classify",
        args: { "x" => BexExternalValue::Int(5) },
    };

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(x: int) -> string {
        load_var x
        jump_table [L4, L4, L3, L3, L2, L2, L1, L1], default L0

      L0:
        load_const "other"
        jump L5

      L1: 7
        load_const "d"
        jump L5

      L2: 5
        load_const "c"
        jump L5

      L3: 3
        load_const "b"
        jump L5

      L4: 1
        load_const "a"

      L5:
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::String("c".to_string())));
}

// ============================================================================
// Range Limit Boundary Tests (256)
// ============================================================================

/// Dense 4 values at high range (252-255) should use jump table.
#[tokio::test]
async fn match_range_at_limit_uses_jump_table() {
    let output = baml_test! {
        baml: r#"
            function classify(x: int) -> int {
                match (x) {
                    252 => 1,
                    253 => 2,
                    254 => 3,
                    255 => 4,
                    _ => 0
                }
            }
        "#,
        entry: "classify",
        args: { "x" => BexExternalValue::Int(254) },
    };

    insta::assert_snapshot!(output.bytecode, @"
    function classify(x: int) -> int {
        load_var x
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 0
        jump L5

      L1: 255
        load_const 4
        jump L5

      L2: 254
        load_const 3
        jump L5

      L3: 253
        load_const 2
        jump L5

      L4: 252
        load_const 1

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

/// Values spanning more than 256 range should NOT use jump table.
#[tokio::test]
async fn match_range_exceeds_limit_uses_binary_search() {
    let output = baml_test! {
        baml: r#"
            function classify(x: int) -> int {
                match (x) {
                    0 => 1,
                    100 => 2,
                    200 => 3,
                    300 => 4,
                    _ => 0
                }
            }
        "#,
        entry: "classify",
        args: { "x" => BexExternalValue::Int(200) },
    };

    insta::assert_snapshot!(output.bytecode, @"
    function classify(x: int) -> int {
        load_var x
        dense_tag [0, 100, 200, 300]
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 0
        jump L5

      L1: 300
        load_const 4
        jump L5

      L2: 200
        load_const 3
        jump L5

      L3: 100
        load_const 2
        jump L5

      L4: 0
        load_const 1

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ============================================================================
// Large Jump Table Tests (10+ arms)
// ============================================================================

/// Large dense integer match with 10 arms should use jump table.
#[tokio::test]
async fn match_large_jump_table() {
    let output = baml_test! {
        baml: r#"
            function classify(x: int) -> int {
                match (x) {
                    0 => 100,
                    1 => 101,
                    2 => 102,
                    3 => 103,
                    4 => 104,
                    5 => 105,
                    6 => 106,
                    7 => 107,
                    8 => 108,
                    9 => 109,
                    _ => 999
                }
            }
        "#,
        entry: "classify",
        args: { "x" => BexExternalValue::Int(7) },
    };

    insta::assert_snapshot!(output.bytecode, @"
    function classify(x: int) -> int {
        load_var x
        jump_table [L10, L9, L8, L7, L6, L5, L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L11

      L1: 9
        load_const 109
        jump L11

      L2: 8
        load_const 108
        jump L11

      L3: 7
        load_const 107
        jump L11

      L4: 6
        load_const 106
        jump L11

      L5: 5
        load_const 105
        jump L11

      L6: 4
        load_const 104
        jump L11

      L7: 3
        load_const 103
        jump L11

      L8: 2
        load_const 102
        jump L11

      L9: 1
        load_const 101
        jump L11

      L10: 0
        load_const 100

      L11:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(107)));
}

// ============================================================================
// Large Binary Search Tree (8 sparse arms)
// ============================================================================

#[tokio::test]
async fn match_binary_search_eight_arms() {
    let output = baml_test! {
        baml: r#"
            function classify(x: int) -> int {
                match (x) {
                    0 => 0,
                    10 => 1,
                    20 => 2,
                    30 => 3,
                    40 => 4,
                    50 => 5,
                    60 => 6,
                    70 => 7,
                    _ => 99
                }
            }
        "#,
        entry: "classify",
        args: { "x" => BexExternalValue::Int(50) },
    };

    insta::assert_snapshot!(output.bytecode, @"
    function classify(x: int) -> int {
        load_var x
        dense_tag [0, 10, 20, 30, 40, 50, 60, 70]
        jump_table [L8, L7, L6, L5, L4, L3, L2, L1], default L0

      L0:
        load_const 99
        jump L9

      L1: 70
        load_const 7
        jump L9

      L2: 60
        load_const 6
        jump L9

      L3: 50
        load_const 5
        jump L9

      L4: 40
        load_const 4
        jump L9

      L5: 30
        load_const 3
        jump L9

      L6: 20
        load_const 2
        jump L9

      L7: 10
        load_const 1
        jump L9

      L8: 0
        load_const 0

      L9:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}
