use internal_baml_core::ir::{Class, Field};

use crate::{
    generated_types::{ClassPy, FieldPy},
    package::CurrentRenderPackage,
};

pub fn ir_class_to_py<'a>(class: &Class, pkg: &'a CurrentRenderPackage) -> ClassPy<'a> {
    ClassPy {
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
    FieldPy {
        name: field.elem.name.clone(),
        r#type: super::type_to_py(
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

fn ir_field_to_py_stream<'a>(field: &Field, pkg: &'a CurrentRenderPackage) -> FieldPy<'a> {
    let partialized = field.elem.r#type.elem.to_streaming_type(pkg.lookup());
    FieldPy {
        name: field.elem.name.clone(),
        r#type: super::stream_type_to_py(&partialized, pkg.lookup()),
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
        assert_eq!(
            class_py.fields[0]
                .r#type
                .meta()
                .map(|m| m.wrap_stream_state),
            Some(true)
        );
        println!("{}", class_py.fields[0]);
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
        eprintln!("{digits_field:?}");
        assert_eq!(
            digits_field.r#type.meta().map(|m| m.wrap_stream_state),
            Some(true)
        );
        assert_eq!(class_py.name, "ChildClass");
        assert_eq!(class_py.fields.len(), 1);
        println!("{}", class_py.fields[0]);
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
}
