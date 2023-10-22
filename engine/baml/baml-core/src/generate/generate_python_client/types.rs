use internal_baml_schema_ast::ast::{FieldArity, FieldType, FunctionArg, Identifier, TypeValue};

use super::{file::File, traits::WithToCode};

impl WithToCode for Identifier {
    fn to_py_string(&self, f: &mut File) -> String {
        self.name.clone()
    }
}

impl WithToCode for TypeValue {
    fn to_py_string(&self, f: &mut File) -> String {
        match self {
            TypeValue::Char | TypeValue::String => "str".to_string(),
            TypeValue::Int => "int".to_string(),
            TypeValue::Float => "float".to_string(),
            TypeValue::Boolean => "bool".to_string(),
        }
    }
}

impl WithToCode for FunctionArg {
    fn to_py_string(&self, f: &mut File) -> String {
        (self.arity, &self.field_type).to_py_string(f)
    }
}

impl WithToCode for (FieldArity, &FieldType) {
    fn to_py_string(&self, f: &mut File) -> String {
        match self.0 {
            FieldArity::Required => match self.1 {
                FieldType::PrimitiveType(s, _) => s.to_py_string(f),
                FieldType::Supported(idn) => idn.to_py_string(f),
                FieldType::Union(types, _) => {
                    f.add_import("typing", "Union");
                    format!(
                        "Union[{}]",
                        types
                            .iter()
                            .map(|(arity, field)| (arity.clone(), field).to_py_string(f))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
                FieldType::Unsupported(..) => panic!("Unsupported field type"),
            },
            FieldArity::List => {
                f.add_import("typing", "List");
                format!("List[{}]", (FieldArity::Required, self.1).to_py_string(f))
            }
            FieldArity::Optional => {
                f.add_import("typing", "Optional");
                format!(
                    "Optional[{}]",
                    (FieldArity::Required, self.1).to_py_string(f)
                )
            }
        }
    }
}
