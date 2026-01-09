use internal_baml_core::ir::Enum;

use crate::package::CurrentRenderPackage;

pub fn ir_enum_to_py(enum_: &Enum, _pkg: &CurrentRenderPackage) -> crate::generated_types::EnumPy {
    crate::generated_types::EnumPy {
        name: enum_.elem.name.clone(),
        values: enum_
            .elem
            .values
            .iter()
            .map(|(val, doc_string)| (val.elem.0.clone(), doc_string.as_ref().map(|d| d.0.clone())))
            .collect(),
        docstring: enum_.elem.docstring.as_ref().map(|d| d.0.clone()),
        dynamic: enum_.attributes.dynamic(),
    }
}

#[cfg(test)]
mod tests {
    use askama::Template;
    use internal_baml_core::ir::{repr::make_test_ir, IRHelper};

    use super::*;

    #[test]
    fn test_enum_basic() {
        let ir = make_test_ir(
            r#"
        enum Status {
            PENDING
            COMPLETED
        }
        "#,
        )
        .expect("Valid IR");
        let ir = std::sync::Arc::new(ir);
        let enum_ = ir.find_enum("Status").unwrap().item;
        let pkg = crate::package::CurrentRenderPackage::new("baml_client", ir.clone(), true);
        let enum_py = ir_enum_to_py(enum_, &pkg);

        assert_eq!(enum_py.name, "Status");
        assert_eq!(enum_py.values[0].0, "PENDING");
        assert_eq!(enum_py.values[1].0, "COMPLETED");

        let rendered = enum_py.render().expect("render enum");
        assert!(rendered.contains("PENDING = \"PENDING\""));
        assert!(rendered.contains("COMPLETED = \"COMPLETED\""));
    }
}
