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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 0
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1:
        load_const 103
        jump L5

      L2:
        load_const 102
        jump L5

      L3:
        load_const 101
        jump L5

      L4:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 2
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1:
        load_const 103
        jump L5

      L2:
        load_const 102
        jump L5

      L3:
        load_const 101
        jump L5

      L4:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 3
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1:
        load_const 103
        jump L5

      L2:
        load_const 102
        jump L5

      L3:
        load_const 101
        jump L5

      L4:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 10
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1:
        load_const 103
        jump L5

      L2:
        load_const 102
        jump L5

      L3:
        load_const 101
        jump L5

      L4:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        unary_op -
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1:
        load_const 103
        jump L5

      L2:
        load_const 102
        jump L5

      L3:
        load_const 101
        jump L5

      L4:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        jump_table [L4, _, L3, _, L2, _, L1], default L0

      L0:
        load_const 999
        jump L5

      L1:
        load_const 106
        jump L5

      L2:
        load_const 104
        jump L5

      L3:
        load_const 102
        jump L5

      L4:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 4
        jump_table [L4, _, L3, _, L2, _, L1], default L0

      L0:
        load_const 999
        jump L5

      L1:
        load_const 106
        jump L5

      L2:
        load_const 104
        jump L5

      L3:
        load_const 102
        jump L5

      L4:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 11
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1:
        load_const 113
        jump L5

      L2:
        load_const 112
        jump L5

      L3:
        load_const 111
        jump L5

      L4:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 5
        jump_table [L8, L7, L6, L5, L4, L3, L2, L1], default L0

      L0:
        load_const 9999
        jump L9

      L1:
        load_const 1007
        jump L9

      L2:
        load_const 1006
        jump L9

      L3:
        load_const 1005
        jump L9

      L4:
        load_const 1004
        jump L9

      L5:
        load_const 1003
        jump L9

      L6:
        load_const 1002
        jump L9

      L7:
        load_const 1001
        jump L9

      L8:
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
            function classify(x int) -> int {
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

    insta::assert_snapshot!(output.bytecode, @r"
    function classify(x: int) -> int {
        load_var x
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1:
        load_const 103
        jump L5

      L2:
        load_const 102
        jump L5

      L3:
        load_const 101
        jump L5

      L4:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 0
        copy 0
        load_const 60
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L5

      L0:
        copy 0
        load_const 60
        cmp_op <
        pop_jump_if_false L2
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L7

      L1:
        copy 0
        load_const 30
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L6

      L2:
        copy 0
        load_const 99
        cmp_op ==
        pop_jump_if_false L3
        pop 1
        jump L4

      L3:
        pop 1
        load_const 999
        jump L8

      L4:
        load_const 199
        jump L8

      L5:
        load_const 160
        jump L8

      L6:
        load_const 130
        jump L8

      L7:
        load_const 100

      L8:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 60
        copy 0
        load_const 60
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L5

      L0:
        copy 0
        load_const 60
        cmp_op <
        pop_jump_if_false L2
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L7

      L1:
        copy 0
        load_const 30
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L6

      L2:
        copy 0
        load_const 99
        cmp_op ==
        pop_jump_if_false L3
        pop 1
        jump L4

      L3:
        pop 1
        load_const 999
        jump L8

      L4:
        load_const 199
        jump L8

      L5:
        load_const 160
        jump L8

      L6:
        load_const 130
        jump L8

      L7:
        load_const 100

      L8:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 99
        copy 0
        load_const 60
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L5

      L0:
        copy 0
        load_const 60
        cmp_op <
        pop_jump_if_false L2
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L7

      L1:
        copy 0
        load_const 30
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L6

      L2:
        copy 0
        load_const 99
        cmp_op ==
        pop_jump_if_false L3
        pop 1
        jump L4

      L3:
        pop 1
        load_const 999
        jump L8

      L4:
        load_const 199
        jump L8

      L5:
        load_const 160
        jump L8

      L6:
        load_const 130
        jump L8

      L7:
        load_const 100

      L8:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 50
        copy 0
        load_const 60
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L5

      L0:
        copy 0
        load_const 60
        cmp_op <
        pop_jump_if_false L2
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L7

      L1:
        copy 0
        load_const 30
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L6

      L2:
        copy 0
        load_const 99
        cmp_op ==
        pop_jump_if_false L3
        pop 1
        jump L4

      L3:
        pop 1
        load_const 999
        jump L8

      L4:
        load_const 199
        jump L8

      L5:
        load_const 160
        jump L8

      L6:
        load_const 130
        jump L8

      L7:
        load_const 100

      L8:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 500
        copy 0
        load_const 300
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L8

      L0:
        copy 0
        load_const 300
        cmp_op <
        pop_jump_if_false L3
        copy 0
        load_const 100
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L10

      L1:
        copy 0
        load_const 100
        cmp_op <
        pop_jump_if_false L2
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L11

      L2:
        copy 0
        load_const 200
        cmp_op ==
        pop_jump_if_false L3
        pop 1
        jump L9

      L3:
        copy 0
        load_const 400
        cmp_op ==
        pop_jump_if_false L4
        pop 1
        jump L7

      L4:
        copy 0
        load_const 500
        cmp_op ==
        pop_jump_if_false L5
        pop 1
        jump L6

      L5:
        pop 1
        load_const 9999
        jump L12

      L6:
        load_const 1500
        jump L12

      L7:
        load_const 1400
        jump L12

      L8:
        load_const 1300
        jump L12

      L9:
        load_const 1200
        jump L12

      L10:
        load_const 1100
        jump L12

      L11:
        load_const 1000

      L12:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 750
        copy 0
        load_const 750
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L7

      L0:
        copy 0
        load_const 750
        cmp_op <
        pop_jump_if_false L2
        copy 0
        load_const 250
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L9

      L1:
        copy 0
        load_const 500
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L8

      L2:
        copy 0
        load_const 1000
        cmp_op ==
        pop_jump_if_false L3
        pop 1
        jump L6

      L3:
        copy 0
        load_const 1250
        cmp_op ==
        pop_jump_if_false L4
        pop 1
        jump L5

      L4:
        pop 1
        load_const 9999
        jump L10

      L5:
        load_const 2250
        jump L10

      L6:
        load_const 2000
        jump L10

      L7:
        load_const 1750
        jump L10

      L8:
        load_const 1500
        jump L10

      L9:
        load_const 1250

      L10:
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
            function classify(x int) -> int {
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

    insta::assert_snapshot!(output.bytecode, @r"
    function classify(x: int) -> int {
        load_var x
        copy 0
        load_const 60
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L5

      L0:
        copy 0
        load_const 60
        cmp_op <
        pop_jump_if_false L2
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L7

      L1:
        copy 0
        load_const 30
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L6

      L2:
        copy 0
        load_const 99
        cmp_op ==
        pop_jump_if_false L3
        pop 1
        jump L4

      L3:
        pop 1
        load_const 999
        jump L8

      L4:
        load_const 199
        jump L8

      L5:
        load_const 160
        jump L8

      L6:
        load_const 130
        jump L8

      L7:
        load_const 100

      L8:
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
            function classify(x int) -> int {
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

    insta::assert_snapshot!(output.bytecode, @r"
    function classify(x: int) -> int {
        load_var x
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L3

      L0:
        copy 0
        load_const 1
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L2

      L1:
        pop 1
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 4
        jump_table [L4, _, L3, _, L2, _, L1], default L0

      L0:
        load_const 999
        jump L5

      L1:
        load_const 106
        jump L5

      L2:
        load_const 104
        jump L5

      L3:
        load_const 102
        jump L5

      L4:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 6
        copy 0
        load_const 6
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L5

      L0:
        copy 0
        load_const 6
        cmp_op <
        pop_jump_if_false L2
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L7

      L1:
        copy 0
        load_const 3
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L6

      L2:
        copy 0
        load_const 9
        cmp_op ==
        pop_jump_if_false L3
        pop 1
        jump L4

      L3:
        pop 1
        load_const 999
        jump L8

      L4:
        load_const 109
        jump L8

      L5:
        load_const 106
        jump L8

      L6:
        load_const 103
        jump L8

      L7:
        load_const 100

      L8:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 4
        jump_table [L5, L4, L3, _, L2, L1], default L0

      L0:
        load_const 999
        jump L6

      L1:
        load_const 105
        jump L6

      L2:
        load_const 104
        jump L6

      L3:
        load_const 102
        jump L6

      L4:
        load_const 101
        jump L6

      L5:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 102
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 9999
        jump L5

      L1:
        load_const 1003
        jump L5

      L2:
        load_const 1002
        jump L5

      L3:
        load_const 1001
        jump L5

      L4:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1000
        copy 0
        load_const 1000
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L5

      L0:
        copy 0
        load_const 1000
        cmp_op <
        pop_jump_if_false L2
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L7

      L1:
        copy 0
        load_const 500
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L6

      L2:
        copy 0
        load_const 1500
        cmp_op ==
        pop_jump_if_false L3
        pop 1
        jump L4

      L3:
        pop 1
        load_const 9999
        jump L8

      L4:
        load_const 4
        jump L8

      L5:
        load_const 3
        jump L8

      L6:
        load_const 2
        jump L8

      L7:
        load_const 1

      L8:
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 0
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1:
        load_const 103
        jump L5

      L2:
        load_const 102
        jump L5

      L3:
        load_const 101
        jump L5

      L4:
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
        function classify(x int) -> string {
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

      L1:
        load_const "zero"
        jump L5

      L2:
        load_const "neg one"
        jump L5

      L3:
        load_const "neg two"
        jump L5

      L4:
        load_const "neg three"

      L5:
        return
    }

    function main() -> string {
        load_const 2
        unary_op -
        call classify
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
        function classify(x int) -> string {
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

      L1:
        load_const "zero"
        jump L5

      L2:
        load_const "neg one"
        jump L5

      L3:
        load_const "neg two"
        jump L5

      L4:
        load_const "neg three"

      L5:
        return
    }

    function main() -> string {
        load_const 5
        call classify
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
        function classify(x int) -> string {
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

      L1:
        load_const "two"
        jump L6

      L2:
        load_const "one"
        jump L6

      L3:
        load_const "zero"
        jump L6

      L4:
        load_const "neg one"
        jump L6

      L5:
        load_const "neg two"

      L6:
        return
    }

    function main() -> string {
        load_const 1
        call classify
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
        function classify(x int) -> string {
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

      L1:
        load_const "two"
        jump L6

      L2:
        load_const "one"
        jump L6

      L3:
        load_const "zero"
        jump L6

      L4:
        load_const "neg one"
        jump L6

      L5:
        load_const "neg two"

      L6:
        return
    }

    function main() -> string {
        load_const 1
        unary_op -
        call classify
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
        function classify(x int) -> string {
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
        copy 0
        load_const -10
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L5

      L0:
        copy 0
        load_const -10
        cmp_op <
        pop_jump_if_false L2
        copy 0
        load_const -100
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L7

      L1:
        copy 0
        load_const -50
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L6

      L2:
        copy 0
        load_const -1
        cmp_op ==
        pop_jump_if_false L3
        pop 1
        jump L4

      L3:
        pop 1
        load_const "other"
        jump L8

      L4:
        load_const "d"
        jump L8

      L5:
        load_const "c"
        jump L8

      L6:
        load_const "b"
        jump L8

      L7:
        load_const "a"

      L8:
        return
    }

    function main() -> string {
        load_const 50
        unary_op -
        call classify
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::String("b".to_string())));
}

#[tokio::test]
async fn match_binary_search_spanning_zero_sparse() {
    let output = baml_test!(
        r#"
        function classify(x int) -> string {
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
        copy 0
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L5

      L0:
        copy 0
        load_const 1
        cmp_op <
        pop_jump_if_false L2
        copy 0
        load_const -100
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L7

      L1:
        copy 0
        load_const -1
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L6

      L2:
        copy 0
        load_const 100
        cmp_op ==
        pop_jump_if_false L3
        pop 1
        jump L4

      L3:
        pop 1
        load_const "other"
        jump L8

      L4:
        load_const "hundred"
        jump L8

      L5:
        load_const "one"
        jump L8

      L6:
        load_const "neg one"
        jump L8

      L7:
        load_const "neg hundred"

      L8:
        return
    }

    function main() -> string {
        load_const 1
        call classify
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
            function classify(x int) -> string {
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

      L1:
        load_const "d"
        jump L5

      L2:
        load_const "c"
        jump L5

      L3:
        load_const "b"
        jump L5

      L4:
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
            function classify(x int) -> int {
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

    insta::assert_snapshot!(output.bytecode, @r"
    function classify(x: int) -> int {
        load_var x
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 0
        jump L5

      L1:
        load_const 4
        jump L5

      L2:
        load_const 3
        jump L5

      L3:
        load_const 2
        jump L5

      L4:
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
            function classify(x int) -> int {
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

    insta::assert_snapshot!(output.bytecode, @r"
    function classify(x: int) -> int {
        load_var x
        copy 0
        load_const 200
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L5

      L0:
        copy 0
        load_const 200
        cmp_op <
        pop_jump_if_false L2
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L7

      L1:
        copy 0
        load_const 100
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L6

      L2:
        copy 0
        load_const 300
        cmp_op ==
        pop_jump_if_false L3
        pop 1
        jump L4

      L3:
        pop 1
        load_const 0
        jump L8

      L4:
        load_const 4
        jump L8

      L5:
        load_const 3
        jump L8

      L6:
        load_const 2
        jump L8

      L7:
        load_const 1

      L8:
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
            function classify(x int) -> int {
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

    insta::assert_snapshot!(output.bytecode, @r"
    function classify(x: int) -> int {
        load_var x
        jump_table [L10, L9, L8, L7, L6, L5, L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L11

      L1:
        load_const 109
        jump L11

      L2:
        load_const 108
        jump L11

      L3:
        load_const 107
        jump L11

      L4:
        load_const 106
        jump L11

      L5:
        load_const 105
        jump L11

      L6:
        load_const 104
        jump L11

      L7:
        load_const 103
        jump L11

      L8:
        load_const 102
        jump L11

      L9:
        load_const 101
        jump L11

      L10:
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
            function classify(x int) -> int {
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

    insta::assert_snapshot!(output.bytecode, @r"
    function classify(x: int) -> int {
        load_var x
        copy 0
        load_const 40
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L11

      L0:
        copy 0
        load_const 40
        cmp_op <
        pop_jump_if_false L4
        copy 0
        load_const 20
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L13

      L1:
        copy 0
        load_const 20
        cmp_op <
        pop_jump_if_false L3
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L15

      L2:
        copy 0
        load_const 10
        cmp_op ==
        pop_jump_if_false L3
        pop 1
        jump L14

      L3:
        copy 0
        load_const 30
        cmp_op ==
        pop_jump_if_false L4
        pop 1
        jump L12

      L4:
        copy 0
        load_const 60
        cmp_op ==
        pop_jump_if_false L5
        pop 1
        jump L9

      L5:
        copy 0
        load_const 60
        cmp_op <
        pop_jump_if_false L6
        copy 0
        load_const 50
        cmp_op ==
        pop_jump_if_false L6
        pop 1
        jump L10

      L6:
        copy 0
        load_const 70
        cmp_op ==
        pop_jump_if_false L7
        pop 1
        jump L8

      L7:
        pop 1
        load_const 99
        jump L16

      L8:
        load_const 7
        jump L16

      L9:
        load_const 6
        jump L16

      L10:
        load_const 5
        jump L16

      L11:
        load_const 4
        jump L16

      L12:
        load_const 3
        jump L16

      L13:
        load_const 2
        jump L16

      L14:
        load_const 1
        jump L16

      L15:
        load_const 0

      L16:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}
