use internal_baml_schema_ast::ast::{FieldType, FunctionArg, Identifier, TypeValue};

use super::{file::File, traits::WithPartial, traits::WithToCode};

impl WithPartial for Identifier {
    fn to_partial_py_string(&self, f: &mut File) -> String {
        match self {
            Identifier::ENV(str, _) => {
                f.add_import("os", "environ");
                format!("environ['{}']", str)
            }
            // References to other classes in imports should always be the Partial ones
            Identifier::Ref(idn, _) => {
                f.add_import(&idn.path.join("."), &format!("{}", &idn.name));
                idn.name.clone()
            }
            Identifier::Local(idn, _) => format!("{}", &idn.to_string()),
            Identifier::String(str, _) => format!("{}", str.to_string()),
            Identifier::Primitive(p, _) => p.to_py_string(f),
            Identifier::Invalid(inv, _) => panic!("Should never show invalid: {}", inv),
        }
    }
}

impl WithPartial for TypeValue {
    fn to_partial_py_string(&self, _f: &mut File) -> String {
        match self {
            TypeValue::Char | TypeValue::String => "str".to_string(),
            TypeValue::Int => "int".to_string(),
            TypeValue::Float => "float".to_string(),
            TypeValue::Bool => "bool".to_string(),
            TypeValue::Null => "None".to_string(),
            TypeValue::Image => "Image".to_string(),
        }
    }
}

impl WithPartial for FunctionArg {
    fn to_partial_py_string(&self, f: &mut File) -> String {
        self.field_type.to_partial_py_string(f)
    }
}

// impl WithPartial for Class {
//     fn to_partial_py_string(&self, f: &mut File) -> String {
//         format!("Partial{}", self.name.to_partial_py_string(f))
//     }
// }

impl WithPartial for FieldType {
    fn to_partial_py_string(&self, f: &mut File) -> String {
        match self {
            FieldType::Identifier(_arity, idn) => {
                let mut repr = idn.to_partial_py_string(f);

                // they are all optional
                f.add_import("typing", "Optional");
                repr = format!("Optional[{}]", repr);

                repr
            }
            FieldType::List(items, dims, _) => {
                // the item cant be optional.
                let mut repr = items.to_py_string(f);
                f.add_import("typing", "List");

                for _ in 0..*dims {
                    repr = format!("List[{}]", repr);
                }

                return repr;
            }
            FieldType::Dictionary(kv, _) => {
                f.add_import("typing", "Dict");
                let repr = format!(
                    "Dict[{}, {}]",
                    kv.0.to_partial_py_string(f),
                    kv.1.to_partial_py_string(f)
                );
                repr
            }
            FieldType::Tuple(_arity, vals, _) => {
                f.add_import("typing", "Tuple");
                let mut repr = format!(
                    "Tuple[{}]",
                    vals.iter()
                        .map(|v| v.to_partial_py_string(f))
                        .collect::<Vec<String>>()
                        .join(", ")
                );

                f.add_import("typing", "Optional");
                repr = format!("Optional[{}]", repr);

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

                // Optional unions are broken at the moment so dont make this worse for now
                if arity.is_optional() {
                    f.add_import("typing", "Optional");
                    repr = format!("Optional[{}]", repr);
                }
                repr
            }
        }
    }
}
