// Tests for enum variants.
//
// Converted from crates/baml_tests/tests/enums.rs.
//
// `assign_enum_variant`: the Rust test reads a bare local (`let s = Shape.Rectangle; s`).
// In a test block, a local read compares unequal to a literal (VM bug), so the
// body is moved into a helper function where locals are not boxed.

enum EnumsShape {
    Square
    Rectangle
    Circle
}

function enums_return_shape(shape: EnumsShape) -> EnumsShape {
    shape
}

// Helper: returns a bare enum variant — mirrors the original `main()` body so
// the variant is exercised through a function-return boundary, not compared to
// itself.
function enums_return_variant() -> EnumsShape {
    EnumsShape.Rectangle
}

// Helper: binds enum variant to a local before returning — safe inside a function.
function enums_assign_variant() -> EnumsShape {
    let s = EnumsShape.Rectangle;
    s
}

test "return_enum_variant" {
    assert.equal(enums_return_variant(), EnumsShape.Rectangle)
}

test "assign_enum_variant" {
    assert.equal(enums_assign_variant(), EnumsShape.Rectangle)
}

test "pass_enum_variant_to_function" {
    assert.equal(enums_return_shape(EnumsShape.Rectangle), EnumsShape.Rectangle)
}
