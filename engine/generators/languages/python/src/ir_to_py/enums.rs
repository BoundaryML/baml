use baml_types::StringOr;
use internal_baml_core::ir::Enum;

use crate::package::CurrentRenderPackage;

/// Extract a static string value from a StringOr.
/// Only `StringOr::Value` is extracted; env vars and jinja expressions are ignored
/// since they can't be resolved at codegen time.
fn extract_static_description(string_or: Option<&StringOr>) -> Option<String> {
    match string_or {
        Some(StringOr::Value(s)) => Some(s.clone()),
        _ => None,
    }
}

pub fn ir_enum_to_py(enum_: &Enum, _pkg: &CurrentRenderPackage) -> crate::generated_types::EnumPy {
    crate::generated_types::EnumPy {
        name: enum_.elem.name.clone(),
        values: enum_
            .elem
            .values
            .iter()
            .map(|(val, doc_string)| {
                (
                    val.elem.0.clone(),
                    doc_string.as_ref().map(|d| d.0.clone()),
                    extract_static_description(val.attributes.description()),
                )
            })
            .collect(),
        docstring: enum_.elem.docstring.as_ref().map(|d| d.0.clone()),
        description: extract_static_description(enum_.attributes.description()),
        dynamic: enum_.attributes.dynamic(),
    }
}

#[cfg(test)]
mod tests {
    use askama::Template;
    use internal_baml_core::ir::{repr::make_test_ir, IRHelper};

    use super::*;

    #[test]
    fn test_enum_value_description_annotation() {
        let ir = make_test_ir(
            r#"
        enum Status {
            PENDING @description("Task is pending")
            COMPLETED @description("Task is completed")
            FAILED
        }
        "#,
        )
        .expect("Valid IR");
        let ir = std::sync::Arc::new(ir);
        let enum_ = ir.find_enum("Status").unwrap().item;
        let pkg = crate::package::CurrentRenderPackage::new("baml_client", ir.clone(), true);
        let enum_py = ir_enum_to_py(enum_, &pkg);

        // First value should have description
        assert_eq!(enum_py.values[0].0, "PENDING");
        assert_eq!(enum_py.values[0].2, Some("Task is pending".to_string()));

        // Second value should have description
        assert_eq!(enum_py.values[1].0, "COMPLETED");
        assert_eq!(enum_py.values[1].2, Some("Task is completed".to_string()));

        // Third value should have no description
        assert_eq!(enum_py.values[2].0, "FAILED");
        assert_eq!(enum_py.values[2].2, None);

        // The rendered output should include descriptions as comments
        let rendered = enum_py.render().expect("render enum");
        assert!(
            rendered.contains("Task is pending"),
            "Expected description comment in output, got: {}",
            rendered
        );
        assert!(
            rendered.contains("Task is completed"),
            "Expected description comment in output, got: {}",
            rendered
        );
    }

    #[test]
    fn test_enum_description_annotation() {
        let ir = make_test_ir(
            r#"
        enum Status {
            PENDING
            COMPLETED
            @@description("Represents the status of a task")
        }
        "#,
        )
        .expect("Valid IR");
        let ir = std::sync::Arc::new(ir);
        let enum_ = ir.find_enum("Status").unwrap().item;
        let pkg = crate::package::CurrentRenderPackage::new("baml_client", ir.clone(), true);
        let enum_py = ir_enum_to_py(enum_, &pkg);

        // Enum should have description
        assert_eq!(
            enum_py.description,
            Some("Represents the status of a task".to_string())
        );

        // The rendered output should include a Python docstring
        let rendered = enum_py.render().expect("render enum");
        assert!(
            rendered.contains("\"\"\"Represents the status of a task\"\"\""),
            "Expected Python docstring in output, got: {}",
            rendered
        );
    }

    #[test]
    fn test_enum_without_descriptions() {
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

        // No descriptions should be present
        assert_eq!(enum_py.description, None);
        assert_eq!(enum_py.values[0].2, None);
        assert_eq!(enum_py.values[1].2, None);
    }
}
