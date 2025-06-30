use internal_baml_core::ir::{Class, Field};

use crate::{
    generated_types::{ClassRb, FieldRb},
    package::CurrentRenderPackage,
};

pub fn ir_class_to_rb<'a>(class: &Class, pkg: &'a CurrentRenderPackage) -> ClassRb<'a> {
    ClassRb {
        name: class.elem.name.clone(),
        docstring: class
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        dynamic: class.attributes.dynamic(),
        pkg,
        fields: class
            .elem
            .static_fields
            .iter()
            .map(|field| ir_field_to_rb(field, pkg))
            .collect(),
    }
}

pub fn ir_class_to_rb_stream<'a>(class: &Class, pkg: &'a CurrentRenderPackage) -> ClassRb<'a> {
    ClassRb {
        name: class.elem.name.clone(),
        docstring: class
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        dynamic: class.attributes.dynamic(),
        pkg,
        fields: class
            .elem
            .static_fields
            .iter()
            .map(|field| ir_field_to_rb_stream(field, pkg))
            .collect(),
    }
}

fn ir_field_to_rb<'a>(field: &Field, pkg: &'a CurrentRenderPackage) -> FieldRb<'a> {
    FieldRb {
        name: field.elem.name.clone(),
        r#type: super::type_to_rb(
            &field.elem.r#type.elem.to_non_streaming_type(pkg.lookup()),
            pkg.lookup(),
        ),
        docstring: field
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        pkg,
    }
}

fn ir_field_to_rb_stream<'a>(field: &Field, pkg: &'a CurrentRenderPackage) -> FieldRb<'a> {
    let partialized = field.elem.r#type.elem.to_streaming_type(pkg.lookup());
    FieldRb {
        name: field.elem.name.clone(),
        r#type: super::stream_type_to_rb(&partialized, pkg.lookup()),
        docstring: field
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        pkg,
    }
}

#[cfg(test)]
mod tests {
    use internal_baml_core::ir::{repr::make_test_ir, IRHelper};

    use super::*;

    #[test]
    #[ignore]
    fn test_ir_class_to_rb() {
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
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone());
        let class_rb = ir_class_to_rb_stream(class, &pkg);
        assert_eq!(class_rb.name, "SimpleClass");
        assert_eq!(class_rb.fields.len(), 1);
        assert_eq!(
            class_rb.fields[0]
                .r#type
                .meta()
                .map(|m| m.wrap_stream_state),
            Some(true)
        );
        println!("{}", class_rb.fields[0]);
    }

    #[test]
    #[ignore]
    fn test_ir_class_to_rb_needed_field() {
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
        let class_rb = ir_class_to_rb_stream(class, &pkg);
        let digits_field = class_rb.fields.iter().find(|f| f.name == "digits").unwrap();
        eprintln!("{digits_field:?}");
        assert_eq!(
            digits_field.r#type.meta().map(|m| m.wrap_stream_state),
            Some(true)
        );
        assert_eq!(class_rb.name, "ChildClass");
        assert_eq!(class_rb.fields.len(), 1);
        println!("{}", class_rb.fields[0]);
    }

    #[test]
    #[ignore]
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
        let class_rb = ir_class_to_rb_stream(class, &pkg);
        assert_eq!(class_rb.fields[0].docstring, Some("ds".to_string()));
    }
}
