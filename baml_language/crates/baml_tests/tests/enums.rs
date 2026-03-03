//! Unified tests for enum variants.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn return_enum_variant() {
    let output = baml_test!(
        r#"
        enum Shape {
            Square
            Rectangle
            Circle
        }

        function main() -> Shape {
            Shape.Rectangle
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> Shape {
        load_const Shape.Rectangle
        alloc_variant Shape
        return
    }
    ");

    assert_eq!(
        output.result,
        Ok(BexExternalValue::variant("Shape", "Rectangle"))
    );
}

#[tokio::test]
async fn assign_enum_variant() {
    let output = baml_test!(
        r#"
        enum Shape {
            Square
            Rectangle
            Circle
        }

        function main() -> Shape {
            let s = Shape.Rectangle;
            s
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> Shape {
        load_const Shape.Rectangle
        alloc_variant Shape
        return
    }
    ");

    assert_eq!(
        output.result,
        Ok(BexExternalValue::variant("Shape", "Rectangle"))
    );
}

#[tokio::test]
async fn pass_enum_variant_to_function() {
    let output = baml_test!(
        r#"
        enum Shape {
            Square
            Rectangle
            Circle
        }

        function return_shape(shape: Shape) -> Shape {
            shape
        }

        function main() -> Shape {
            return_shape(Shape.Rectangle)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> Shape {
        load_const Shape.Rectangle
        alloc_variant Shape
        call return_shape
        return
    }

    function return_shape(shape: Shape) -> Shape {
        load_var shape
        return
    }
    ");

    assert_eq!(
        output.result,
        Ok(BexExternalValue::variant("Shape", "Rectangle"))
    );
}
