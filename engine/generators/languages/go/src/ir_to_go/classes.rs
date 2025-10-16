use internal_baml_core::ir::{Class, Field};

use crate::{
    generated_types::{ClassGo, FieldGo},
    package::CurrentRenderPackage,
};

pub fn ir_class_to_go<'a>(class: &Class, pkg: &'a CurrentRenderPackage) -> ClassGo<'a> {
    ClassGo {
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
            .map(|field| ir_field_to_go(field, pkg))
            .collect(),
    }
}

pub fn ir_class_to_go_stream<'a>(class: &Class, pkg: &'a CurrentRenderPackage) -> ClassGo<'a> {
    ClassGo {
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
            .map(|field| ir_field_to_go_stream(field, pkg))
            .collect(),
    }
}

fn ir_field_to_go<'a>(field: &Field, pkg: &'a CurrentRenderPackage) -> FieldGo<'a> {
    let non_streaming = field.elem.r#type.elem.to_non_streaming_type(pkg.lookup());
    let go_type = super::type_to_go(&non_streaming, pkg.lookup());
    FieldGo {
        name: field.elem.name.clone(),
        r#type: go_type,
        docstring: field
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        pkg,
    }
}

fn ir_field_to_go_stream<'a>(field: &Field, pkg: &'a CurrentRenderPackage) -> FieldGo<'a> {
    let partialized = field.elem.r#type.elem.to_streaming_type(pkg.lookup());

    FieldGo {
        name: field.elem.name.clone(),
        r#type: super::stream_type_to_go(&partialized, pkg.lookup()),
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
    fn test_ir_class_to_go() {
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
        let class_go = ir_class_to_go_stream(class, &pkg);
        assert_eq!(class_go.name, "SimpleClass");
        assert_eq!(class_go.fields.len(), 1);
        assert!(class_go.fields[0].r#type.meta().wrap_stream_state);
    }

    #[test]
    fn test_ir_class_to_go_needed_field() {
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
        let class_go = ir_class_to_go_stream(class, &pkg);
        let digits_field = class_go.fields.iter().find(|f| f.name == "digits").unwrap();
        assert!(digits_field.r#type.meta().wrap_stream_state);
        assert_eq!(class_go.name, "ChildClass");
        assert_eq!(class_go.fields.len(), 1);
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
        let class_go = ir_class_to_go_stream(class, &pkg);
        assert_eq!(class_go.fields[0].docstring, Some("ds".to_string()));
    }
}
