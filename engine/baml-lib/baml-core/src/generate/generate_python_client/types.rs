use internal_baml_schema_ast::ast::{FieldType, FunctionArg, Identifier, TypeValue};

use super::{file::File, traits::WithToCode};

impl WithToCode for Identifier {
    fn to_py_string(&self, f: &mut File) -> String {
        match self {
            Identifier::ENV(str, _) => {
                f.add_import("os", "environ");
                format!("environ['{}']", str)
            }
            Identifier::Ref(idn, _) => {
                f.add_import(&idn.path.join("."), &idn.name);
                idn.name.clone()
            }
            Identifier::Local(idn, _) => idn.into(),
            Identifier::String(str, _) => str.into(),
            Identifier::Primitive(p, _) => p.to_py_string(f),
            Identifier::Invalid(inv, _) => panic!("Should never show invalid: {}", inv),
        }
    }
}

impl WithToCode for TypeValue {
    fn to_py_string(&self, _f: &mut File) -> String {
        match self {
            TypeValue::Char | TypeValue::String => "str".to_string(),
            TypeValue::Int => "int".to_string(),
            TypeValue::Float => "float".to_string(),
            TypeValue::Bool => "bool".to_string(),
            TypeValue::Null => "None".to_string(),
        }
    }
}

impl WithToCode for FunctionArg {
    fn to_py_string(&self, f: &mut File) -> String {
        self.field_type.to_py_string(f)
    }
}

impl WithToCode for FieldType {
    fn to_py_string(&self, f: &mut File) -> String {
        match self {
            FieldType::Identifier(arity, idn) => {
                let mut repr = idn.to_py_string(f);
                if arity.is_optional() {
                    f.add_import("typing", "Optional");
                    repr = format!("Optional[{}]", repr);
                }
                repr
            }
            FieldType::List(items, dims, _) => {
                let mut repr = items.to_py_string(f);
                f.add_import("typing", "List");

                for _ in 0..*dims {
                    repr = format!("List[{}]", repr);
                }

                return repr;
            }
            FieldType::Dictionary(kv, _) => {
                f.add_import("typing", "Dict");
                let repr = format!("Dict[{}, {}]", kv.0.to_py_string(f), kv.1.to_py_string(f));
                repr
            }
            FieldType::Tuple(arity, vals, _) => {
                f.add_import("typing", "Tuple");
                let mut repr = format!(
                    "Tuple[{}]",
                    vals.iter()
                        .map(|v| v.to_py_string(f))
                        .collect::<Vec<String>>()
                        .join(", ")
                );
                if arity.is_optional() {
                    f.add_import("typing", "Optional");
                    repr = format!("Optional[{}]", repr);
                }
                repr
            }
            FieldType::Union(arity, vals, _) => {
                f.add_import("typing", "Union");
                let mut repr = format!(
                    "Union[{}]",
                    vals.iter()
                        .map(|v| v.to_py_string(f))
                        .collect::<Vec<String>>()
                        .join(", ")
                );
                if arity.is_optional() {
                    f.add_import("typing", "Optional");
                    repr = format!("Optional[{}]", repr);
                }
                repr
            }
        }
    }
}
