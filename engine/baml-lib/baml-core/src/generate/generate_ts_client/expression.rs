use crate::generate::ir::{Expression, Identifier, TypeValue};

use super::ts_language_features::ToTypeScript;

impl ToTypeScript for Expression {
    fn to_ts(&self) -> String {
        match self {
            Expression::List(values) => {
                format!(
                    "[{}]",
                    values
                        .iter()
                        .map(|v| v.to_ts())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Expression::Map(values) => {
                format!(
                    "{{ {} }}",
                    values
                        .iter()
                        .map(|(k, v)| format!("{}: {}", k.to_ts(), v.to_ts()))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Expression::Identifier(idn) => match idn {
                Identifier::ENV(idn) => format!("process.env.{}", idn),
                Identifier::Local(k) => format!("\"{}\"", k.replace('"', "\\\"")),
                Identifier::Ref(r) => format!("\"{}\"", r.join(".")),
                Identifier::Primitive(p) => p.to_ts(),
            },
            Expression::String(val) => format!("\"{}\"", val.escape_default()),
            Expression::RawString(val) => format!("`{}`", val.replace('`', "\\`")),
            Expression::Numeric(val) => val.clone(),
            Expression::Bool(val) => val.to_string(),
        }
    }
}

impl ToTypeScript for TypeValue {
    fn to_ts(&self) -> String {
        match self {
            TypeValue::Bool => "boolean".to_string(),
            TypeValue::Float => "number".to_string(),
            TypeValue::Int => "number".to_string(),
            TypeValue::String => "string".to_string(),
            TypeValue::Null => "null".to_string(),
            TypeValue::Image => "Image".to_string(),
        }
    }
}
