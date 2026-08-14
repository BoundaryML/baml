use baml_types::StringOr;
use internal_baml_core::ir::{Class, Field};

use crate::{
    generated_types::{ClassPy, FieldPy},
    package::CurrentRenderPackage,
    r#type::EscapedPythonString,
};

/// Extract a static string value from a StringOr.
/// Only `StringOr::Value` is extracted; env vars and jinja expressions are ignored
/// since they can't be resolved at codegen time.
fn extract_static_description(string_or: Option<&StringOr>) -> Option<String> {
    match string_or {
        Some(StringOr::Value(s)) => Some(s.clone()),
        _ => None,
    }
}

pub fn ir_class_to_py<'a>(class: &Class, pkg: &'a CurrentRenderPackage) -> ClassPy<'a> {
    ClassPy {
        name: class.elem.name.clone(),
        docstring: class
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        description: extract_static_description(class.attributes.description()),
        dynamic: class.attributes.dynamic(),
        pkg,
        fields: class
            .elem
            .static_fields
            .iter()
            .map(|field| ir_field_to_py(field, pkg))
            .collect(),
    }
}

pub fn ir_class_to_py_stream<'a>(class: &Class, pkg: &'a CurrentRenderPackage) -> ClassPy<'a> {
    ClassPy {
        name: class.elem.name.clone(),
        docstring: class
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        description: extract_static_description(class.attributes.description()),
        dynamic: class.attributes.dynamic(),
        pkg,
        fields: class
            .elem
            .static_fields
            .iter()
            .map(|field| ir_field_to_py_stream(field, pkg))
            .collect(),
    }
}

fn ir_field_to_py<'a>(field: &Field, pkg: &'a CurrentRenderPackage) -> FieldPy<'a> {
    let field_ty = field.elem.r#type.elem.to_non_streaming_type(pkg.lookup());
    let r#type = super::type_to_py(&field_ty, pkg.lookup());

    FieldPy {
        name: field.elem.name.clone(),
        r#type,
        docstring: field
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        description: extract_static_description(field.attributes.description())
            .map(|s| EscapedPythonString::new(&s)),
        pkg,
    }
}

fn ir_field_to_py_stream<'a>(field: &Field, pkg: &'a CurrentRenderPackage) -> FieldPy<'a> {
    let partialized = field.elem.r#type.elem.to_streaming_type(pkg.lookup());
    let r#type = super::stream_type_to_py(&partialized, pkg.lookup());

    FieldPy {
        name: field.elem.name.clone(),
        r#type,
        docstring: field
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        description: extract_static_description(field.attributes.description())
            .map(|s| EscapedPythonString::new(&s)),
        pkg,
    }
}

#[cfg(test)]
mod tests {
    use internal_baml_core::ir::{repr::make_test_ir, IRHelper};

    use super::*;
    use crate::r#type::TypePy;

    #[test]
    fn test_ir_class_to_py() {
        let ir: dir_writer::IntermediateRepr = make_test_ir(
            r#"
            class SimpleClass {
                words string @stream.with_state
            }
        "#,
        )
        .unwrap();
        let ir = std::sync::Arc::new(ir);
        let class = ir.find_class("SimpleClass").unwrap().item;
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone(), true);
        let class_py = ir_class_to_py_stream(class, &pkg);
        assert_eq!(class_py.name, "SimpleClass");
        assert_eq!(class_py.fields.len(), 1);
        assert!(
            matches!(class_py.fields[0].r#type, TypePy::StreamState(_)),
            "Expected StreamState wrapper"
        );
    }

    #[test]
    fn test_ir_class_to_py_needed_field() {
        let ir = make_test_ir(
            r#"
            class ChildClass {
                digits int @stream.with_state @stream.not_null
            }
        "#,
        )
        .unwrap();
        let ir = std::sync::Arc::new(ir);
        let class = ir.find_class("ChildClass").unwrap().item;
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone(), true);
        let class_py = ir_class_to_py_stream(class, &pkg);
        let digits_field = class_py.fields.iter().find(|f| f.name == "digits").unwrap();
        assert!(
            matches!(digits_field.r#type, TypePy::StreamState(_)),
            "Expected StreamState wrapper"
        );
        assert_eq!(class_py.name, "ChildClass");
        assert_eq!(class_py.fields.len(), 1);
    }

    #[test]
    fn test_class_with_field_docstring() {
        let ir = make_test_ir(
            r#"
        class Foo {
            /// ds
            bar string @description("d")
        }
        "#,
        )
        .expect("Valid IR");
        let ir = std::sync::Arc::new(ir);
        let class = ir.find_class("Foo").unwrap().item;
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone(), true);
        let class_py = ir_class_to_py_stream(class, &pkg);
        assert_eq!(class_py.fields[0].docstring, Some("ds".to_string()));
    }

    #[test]
    fn test_field_description_annotation() {
        use askama::Template;

        use crate::r#type::EscapedPythonString;

        let ir = make_test_ir(
            r#"
        class Foo {
            bar string @description("This is the bar field")
        }
        "#,
        )
        .expect("Valid IR");
        let ir = std::sync::Arc::new(ir);
        let class = ir.find_class("Foo").unwrap().item;
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone(), true);
        let class_py = ir_class_to_py(class, &pkg);

        // The description should be captured (as EscapedPythonString)
        assert_eq!(
            class_py.fields[0].description,
            Some(EscapedPythonString::new("This is the bar field"))
        );

        // The rendered output should use Pydantic Field(description=...)
        let rendered = class_py.fields[0].render().expect("render field");
        assert!(
            rendered.contains("Field("),
            "Expected Field() in output, got: {}",
            rendered
        );
        assert!(
            rendered.contains("description="),
            "Expected description= in output, got: {}",
            rendered
        );
    }

    #[test]
    fn test_field_without_description() {
        use askama::Template;

        let ir = make_test_ir(
            r#"
        class Foo {
            bar string
        }
        "#,
        )
        .expect("Valid IR");
        let ir = std::sync::Arc::new(ir);
        let class = ir.find_class("Foo").unwrap().item;
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone(), true);
        let class_py = ir_class_to_py(class, &pkg);

        // No description should be captured
        assert_eq!(class_py.fields[0].description, None);

        // The rendered output should NOT use Field() when there's no description
        let rendered = class_py.fields[0].render().expect("render field");
        assert!(
            !rendered.contains("Field("),
            "Expected no Field() in output when no description, got: {}",
            rendered
        );
    }

    #[test]
    fn test_class_description_annotation() {
        use askama::Template;

        let ir = make_test_ir(
            r#"
        class Foo {
            bar string
            @@description("This is the Foo class")
        }
        "#,
        )
        .expect("Valid IR");
        let ir = std::sync::Arc::new(ir);
        let class = ir.find_class("Foo").unwrap().item;
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone(), true);
        let class_py = ir_class_to_py(class, &pkg);

        // The class description should be captured
        assert_eq!(
            class_py.description,
            Some("This is the Foo class".to_string())
        );

        // The rendered output should include a Python docstring
        let rendered = class_py.render().expect("render class");
        assert!(
            rendered.contains(r#""""This is the Foo class""""#)
                || rendered.contains("\"\"\"This is the Foo class\"\"\""),
            "Expected Python docstring in output, got: {}",
            rendered
        );
    }

    #[test]
    fn test_class_without_description() {
        let ir = make_test_ir(
            r#"
        class Foo {
            bar string
        }
        "#,
        )
        .expect("Valid IR");
        let ir = std::sync::Arc::new(ir);
        let class = ir.find_class("Foo").unwrap().item;
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone(), true);
        let class_py = ir_class_to_py(class, &pkg);

        // No description should be captured (only docstring from comments)
        assert_eq!(class_py.description, None);
    }

    #[test]
    fn test_multiline_class_description() {
        use askama::Template;

        let ir = make_test_ir(
            r##"
        class Foo {
            bar string
            @@description(#"
                This is a multiline description.
                It has multiple lines.
                And should be dedented properly.
            "#)
        }
        "##,
        )
        .expect("Valid IR");
        let ir = std::sync::Arc::new(ir);
        let class = ir.find_class("Foo").unwrap().item;
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone(), true);
        let class_py = ir_class_to_py(class, &pkg);

        // The description should be dedented
        let desc = class_py
            .description
            .as_ref()
            .expect("should have description");
        assert!(
            !desc.starts_with(" ") && !desc.starts_with("\t"),
            "Description should be dedented, got: {:?}",
            desc
        );
        assert!(
            desc.contains("This is a multiline description."),
            "Expected description content, got: {:?}",
            desc
        );

        // Check the rendered output has properly indented docstring
        let rendered = class_py.render().expect("render class");
        assert!(
            rendered.contains("    It has multiple lines."),
            "Subsequent lines of docstring should be indented, got:\n{}",
            rendered
        );
    }

    #[test]
    fn test_field_description_edge_cases() {
        use askama::Template;

        let ir = make_test_ir(
            r##"
        class Foo {
            multiline string @description(#"
                This field has a
                multiline description.
            "#)
            single_quotes string @description("It's a test with 'quotes'")
            backslashes string @description("Path: C:\\Users\\test")
            tabs_and_newlines string @description(#"Tab:	and newline:
end"#)
            triple_quotes string @description(#"Has """triple quotes""" inside"#)
            empty string @description("")
        }
        "##,
        )
        .expect("Valid IR");
        let ir = std::sync::Arc::new(ir);
        let class = ir.find_class("Foo").unwrap().item;
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone(), true);
        let class_py = ir_class_to_py(class, &pkg);

        let rendered = class_py.render().expect("render class");

        // Multiline: should use escaped newlines, not triple quotes
        assert!(
            rendered.contains(r"description='This field has a\nmultiline description.'"),
            "Field description should use escaped newlines, got:\n{}",
            rendered
        );
        assert!(
            !rendered.contains(r#"description=""""#),
            "Field description should not use triple quotes, got:\n{}",
            rendered
        );

        // Single quotes: should be escaped
        assert!(
            rendered.contains(r"description='It\'s a test with \'quotes\''"),
            "Single quotes should be escaped, got:\n{}",
            rendered
        );

        // Backslashes: should be escaped
        assert!(
            rendered.contains(r"description='Path: C:\\Users\\test'"),
            "Backslashes should be escaped, got:\n{}",
            rendered
        );

        // Tab and newline: should be escaped
        assert!(
            rendered.contains(r"\t") && rendered.contains(r"\n"),
            "Tab and newline should be escaped, got:\n{}",
            rendered
        );

        // Triple quotes inside: should be present (Python single-quoted strings handle this fine)
        assert!(
            rendered.contains("triple quotes"),
            "Triple quotes field should be present, got:\n{}",
            rendered
        );

        // Empty: should still generate valid Field()
        assert!(
            rendered.contains("description=''"),
            "Empty description should generate empty string, got:\n{}",
            rendered
        );
    }

    #[test]
    fn test_class_description_with_triple_quotes() {
        use askama::Template;

        let ir = make_test_ir(
            r##"
        class Foo {
            bar string
            @@description(#"This has """triple quotes""" inside"#)
        }
        "##,
        )
        .expect("Valid IR");
        let ir = std::sync::Arc::new(ir);
        let class = ir.find_class("Foo").unwrap().item;
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone(), true);
        let class_py = ir_class_to_py(class, &pkg);

        let rendered = class_py.render().expect("render class");
        // The generated code should be valid Python (triple quotes must be escaped)
        // Check that we don't have unbalanced triple quotes
        let triple_quote_count = rendered.matches(r#"""""#).count();
        assert!(
            triple_quote_count % 2 == 0,
            "Triple quotes should be balanced in output, got:\n{}",
            rendered
        );
    }

    #[test]
    fn test_model_rebuilds_emitted_for_every_class() {
        // Regression for issue #793: `A` composes `B` but sorts first
        // alphabetically, so its `"B"` forward ref is unresolved at definition
        // time. A trailing rebuild for every model fixes resolution regardless
        // of order.
        let ir = make_test_ir(
            r#"
            class B {
                value string
            }
            class A {
                property1 B
            }
        "#,
        )
        .unwrap();
        let ir = std::sync::Arc::new(ir);
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone(), true);
        let classes = vec![
            ir_class_to_py(ir.find_class("A").unwrap().item, &pkg),
            ir_class_to_py(ir.find_class("B").unwrap().item, &pkg),
        ];

        // No recursive classes here, so both A and B get a direct rebuild.
        let recursive = std::collections::HashSet::new();
        let rendered =
            crate::generated_types::render_py_model_rebuilds(&classes, &recursive, &pkg).unwrap();
        assert!(
            rendered.contains("A.model_rebuild()"),
            "expected A.model_rebuild(), got:\n{rendered}"
        );
        assert!(
            rendered.contains("B.model_rebuild()"),
            "expected B.model_rebuild(), got:\n{rendered}"
        );
        assert!(
            !rendered.contains("update_forward_refs"),
            "pydantic 2 should use model_rebuild, got:\n{rendered}"
        );
    }

    #[test]
    fn test_model_rebuilds_skip_recursive_classes() {
        // Recursive classes must be omitted (Pydantic resolves them lazily;
        // eagerly rebuilding can recurse — issue #793).
        let ir = make_test_ir(
            r#"
            class B {
                value string
            }
            class A {
                property1 B
            }
        "#,
        )
        .unwrap();
        let ir = std::sync::Arc::new(ir);
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone(), true);
        let classes = vec![
            ir_class_to_py(ir.find_class("A").unwrap().item, &pkg),
            ir_class_to_py(ir.find_class("B").unwrap().item, &pkg),
        ];

        // Pretend A is recursive: it should be excluded, B should remain.
        let recursive = std::collections::HashSet::from(["A".to_string()]);
        let rendered =
            crate::generated_types::render_py_model_rebuilds(&classes, &recursive, &pkg).unwrap();
        assert!(
            !rendered.contains("A.model_rebuild()"),
            "recursive class A should be skipped, got:\n{rendered}"
        );
        assert!(
            rendered.contains("B.model_rebuild()"),
            "non-recursive class B should be rebuilt, got:\n{rendered}"
        );
    }

    #[test]
    fn test_model_rebuilds_use_update_forward_refs_on_pydantic_1() {
        let ir = make_test_ir(
            r#"
            class A {
                value string
            }
        "#,
        )
        .unwrap();
        let ir = std::sync::Arc::new(ir);
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone(), false);
        let classes = vec![ir_class_to_py(ir.find_class("A").unwrap().item, &pkg)];

        let recursive = std::collections::HashSet::new();
        let rendered =
            crate::generated_types::render_py_model_rebuilds(&classes, &recursive, &pkg).unwrap();
        assert!(
            rendered.contains("A.update_forward_refs()"),
            "pydantic 1 should use update_forward_refs, got:\n{rendered}"
        );
        assert!(!rendered.contains("model_rebuild"));
    }

    #[test]
    fn test_model_rebuilds_empty_when_no_classes() {
        let ir = make_test_ir(r#"class A { value string }"#).unwrap();
        let ir = std::sync::Arc::new(ir);
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone(), true);

        let recursive = std::collections::HashSet::new();
        let rendered =
            crate::generated_types::render_py_model_rebuilds(&[], &recursive, &pkg).unwrap();
        assert!(
            !rendered.contains("model_rebuild") && !rendered.contains("Model rebuilds"),
            "no classes should render nothing, got:\n{rendered}"
        );
    }

    #[test]
    fn test_class_description_with_backslash_at_end() {
        use askama::Template;

        let ir = make_test_ir(
            r##"
        class Foo {
            bar string
            @@description("Ends with backslash\\")
        }
        "##,
        )
        .expect("Valid IR");
        let ir = std::sync::Arc::new(ir);
        let class = ir.find_class("Foo").unwrap().item;
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone(), true);
        let class_py = ir_class_to_py(class, &pkg);

        let rendered = class_py.render().expect("render class");
        // A backslash at the end of a docstring line could escape the closing quotes
        // This test ensures the output is valid
        assert!(
            rendered.contains("Ends with backslash"),
            "Description should be present, got:\n{}",
            rendered
        );
    }
}
