use internal_baml_core::ir::{FieldType, TypeValue};

use super::python_language_features::ToPython;

impl ToPython for FieldType {
    fn to_python(&self) -> String {
        match self {
            FieldType::Class(name) | FieldType::Enum(name) => format!("\"{name}\""),
            FieldType::List(inner) => format!("List[{}]", inner.to_python()),
            FieldType::Map(key, value) => {
                format!("Dict[{}, {}]", key.to_python(), value.to_python())
            }
            FieldType::Primitive(r#type) => r#type.to_python(),
            FieldType::Union(inner) => format!(
                "Union[{}]",
                inner
                    .iter()
                    .map(|t| t.to_python())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Tuple(inner) => format!(
                "Tuple[{}]",
                inner
                    .iter()
                    .map(|t| t.to_python())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Optional(inner) => format!("Optional[{}]", inner.to_python()),
        }
    }
}
