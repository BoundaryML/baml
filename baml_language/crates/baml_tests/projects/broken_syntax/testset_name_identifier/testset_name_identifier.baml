// A bare identifier as a top-level test/testset name can never resolve —
// there are no local bindings at top level. The parser reports the syntax
// fix ("names must be quoted strings") instead of leaving the user with a
// misleading downstream E0003 "unresolved name".

testset foo {
    test "inner" {
        assert.is_true(true)
    }
}

test my_test {
    assert.is_true(true)
}

// Quoted names are fine, and identifiers remain legal in nested position
// where a loop variable is in scope.
testset "quoted" {
    for (let n in ["a", "b"]) {
        test n {
            assert.is_true(true)
        }
    }
}
