use internal_baml_schema_ast::ast::{FieldType, FunctionArg};

use super::{file::File, traits::WithToCode};

impl WithToCode for FunctionArg {
    fn to_py_string(&self, f: &mut File) -> String {
        self.field_type.to_py_string(f)
    }

    fn to_ts_string(&self, f: &mut super::file::File) -> String {
        self.field_type.to_ts_string(f)
    }
}

impl WithToCode for FieldType {
    fn to_ts_string(&self, f: &mut super::file::File) -> String {
        todo!()
    }

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
