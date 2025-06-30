use internal_baml_core::ir::{Class, Field};

use crate::{
    generated_types::{ClassTS, FieldTS},
    package::CurrentRenderPackage,
};

pub fn ir_class_to_ts<'a>(class: &Class, pkg: &'a CurrentRenderPackage) -> ClassTS<'a> {
    ClassTS {
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
            .map(|field| ir_field_to_ts(field, pkg))
            .collect(),
    }
}

pub fn ir_class_to_ts_stream<'a>(class: &Class, pkg: &'a CurrentRenderPackage) -> ClassTS<'a> {
    ClassTS {
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
            .map(|field| ir_field_to_ts_stream(field, pkg))
            .collect(),
    }
}

fn ir_field_to_ts<'a>(field: &Field, pkg: &'a CurrentRenderPackage) -> FieldTS<'a> {
    FieldTS {
        name: field.elem.name.clone(),
        r#type: super::type_to_ts(
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

fn ir_field_to_ts_stream<'a>(field: &Field, pkg: &'a CurrentRenderPackage) -> FieldTS<'a> {
    let partialized = field.elem.r#type.elem.to_streaming_type(pkg.lookup());

    FieldTS {
        name: field.elem.name.clone(),
        r#type: super::stream_type_to_ts(&partialized, pkg.lookup()),
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
    fn test_ir_class_to_ts() {
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
        let class_go = ir_class_to_ts_stream(class, &pkg);
        assert_eq!(class_go.name, "SimpleClass");
        assert_eq!(class_go.fields.len(), 1);
        assert!(class_go.fields[0].r#type.meta().wrap_stream_state);
        println!("{}", class_go.fields[0]);
    }

    #[test]
    fn test_ir_class_to_ts_needed_field() {
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
        let class_ts = ir_class_to_ts_stream(class, &pkg);
        let digits_field = class_ts.fields.iter().find(|f| f.name == "digits").unwrap();
        eprintln!("{digits_field:?}");
        eprintln!("{}", class.elem.static_fields[0].elem.r#type.elem);
        eprintln!(
            "{}",
            class.elem.static_fields[0]
                .elem
                .r#type
                .elem
                .to_streaming_type(ir.as_ref())
        );
        assert!(digits_field.r#type.meta().wrap_stream_state);
        assert_eq!(class_ts.name, "ChildClass");
        assert_eq!(class_ts.fields.len(), 1);
        println!("{}", class_ts.fields[0]);
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
        let class_ts = ir_class_to_ts_stream(class, &pkg);
        assert_eq!(class_ts.fields[0].docstring, Some("ds".to_string()));
    }
}
