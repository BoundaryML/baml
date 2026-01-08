use std::collections::HashSet;

use internal_baml_core::ir::{Class, Field};

use crate::{
    generated_types::{ClassRust, FieldRust},
    package::CurrentRenderPackage,
    RecursiveCycles,
};

pub fn ir_class_to_rust(
    class: &Class,
    pkg: &CurrentRenderPackage,
    cycles: &RecursiveCycles,
) -> ClassRust {
    let class_name = &class.elem.name;
    // Find which cycle this class belongs to (if any)
    let containing_cycle = cycles.iter().find(|c| c.contains(class_name));

    ClassRust {
        name: class.elem.name.clone(),
        docstring: class
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        dynamic: class.attributes.dynamic(),
        fields: class
            .elem
            .static_fields
            .iter()
            .map(|field| ir_field_to_rust(field, pkg, containing_cycle))
            .collect(),
    }
}

pub fn ir_class_to_rust_stream(
    class: &Class,
    pkg: &CurrentRenderPackage,
    cycles: &RecursiveCycles,
) -> ClassRust {
    let class_name = &class.elem.name;
    // Find which cycle this class belongs to (if any)
    let containing_cycle = cycles.iter().find(|c| c.contains(class_name));

    ClassRust {
        name: class.elem.name.clone(),
        docstring: class
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        dynamic: class.attributes.dynamic(),
        fields: class
            .elem
            .static_fields
            .iter()
            .map(|field| ir_field_to_rust_stream(field, pkg, containing_cycle))
            .collect(),
    }
}

fn ir_field_to_rust(
    field: &Field,
    pkg: &CurrentRenderPackage,
    containing_cycle: Option<&HashSet<String>>,
) -> FieldRust {
    let non_streaming = field.elem.r#type.elem.to_non_streaming_type(pkg.lookup());
    let rust_type = super::type_to_rust(&non_streaming, pkg.lookup(), containing_cycle);

    FieldRust::new(
        &field.elem.name,
        field
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        rust_type,
    )
}

fn ir_field_to_rust_stream(
    field: &Field,
    pkg: &CurrentRenderPackage,
    containing_cycle: Option<&HashSet<String>>,
) -> FieldRust {
    let partialized = field.elem.r#type.elem.to_streaming_type(pkg.lookup());
    let rust_type = super::stream_type_to_rust(&partialized, pkg.lookup(), containing_cycle);

    FieldRust::new(
        &field.elem.name,
        field
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        rust_type,
    )
}

#[cfg(test)]
mod tests {
    use internal_baml_core::ir::{repr::make_test_ir, IRHelper};

    use super::*;
    use crate::r#type::TypeRust;

    #[test]
    fn test_ir_class_to_rust() {
        let ir = make_test_ir(
            r#"
            class SimpleClass {
                words string @stream.with_state
            }
        "#,
        )
        .unwrap();
        let ir = std::sync::Arc::new(ir);
        let class = ir.find_class("SimpleClass").unwrap().item;
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone());
        let cycles = vec![]; // No recursive cycles in this test
        let class_rust = ir_class_to_rust_stream(class, &pkg, &cycles);
        assert_eq!(class_rust.name, "SimpleClass");
        assert_eq!(class_rust.fields.len(), 1);
        assert!(matches!(
            class_rust.fields[0].r#type,
            TypeRust::StreamState(_)
        ));
    }

    #[test]
    fn test_ir_class_to_rust_needed_field() {
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
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone());
        let cycles = vec![]; // No recursive cycles in this test
        let class_rust = ir_class_to_rust_stream(class, &pkg, &cycles);
        let digits_field = class_rust
            .fields
            .iter()
            .find(|f| f.name() == "digits")
            .unwrap();
        assert!(matches!(digits_field.r#type, TypeRust::StreamState(_)));
        assert_eq!(class_rust.name, "ChildClass");
        assert_eq!(class_rust.fields.len(), 1);
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
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone());
        let cycles = vec![]; // No recursive cycles in this test
        let class_rust = ir_class_to_rust_stream(class, &pkg, &cycles);
        assert_eq!(class_rust.fields[0].docstring, Some("ds".to_string()));
    }
}
