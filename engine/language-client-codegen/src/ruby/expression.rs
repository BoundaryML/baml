use internal_baml_core::ir::{Expression, Identifier, TypeValue};

use super::ruby_language_features::ToRuby;

impl ToRuby for Expression {
    fn to_ruby(&self) -> String {
        match self {
            Expression::List(values) => {
                format!(
                    "[{}]",
                    values
                        .iter()
                        .map(|v| v.to_ruby())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Expression::Map(values) => {
                format!(
                    "{{ {} }}",
                    values
                        .iter()
                        .map(|(k, v)| format!("{}: {}", k.to_ruby(), v.to_ruby()))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Expression::Identifier(idn) => match idn {
                Identifier::ENV(idn) => format!("process.env.{}", idn),
                Identifier::Local(k) => format!("\"{}\"", k.replace('"', "\\\"")),
                Identifier::Ref(r) => format!("\"{}\"", r.join(".")),
                Identifier::Primitive(p) => p.to_ruby(),
            },
            Expression::String(val) => format!("\"{}\"", val.escape_default()),
            Expression::RawString(val) => format!("`{}`", val.replace('`', "\\`")),
            Expression::Numeric(val) => val.clone(),
            Expression::Bool(val) => val.to_string(),
        }
    }
}

impl ToRuby for TypeValue {
    fn to_ruby(&self) -> String {
        match self {
            TypeValue::Bool => "boolean".to_string(),
            TypeValue::Float => "number".to_string(),
            TypeValue::Int => "number".to_string(),
            TypeValue::String => "string".to_string(),
            TypeValue::Null => "null".to_string(),
            TypeValue::Image => "Image".to_string(),
            TypeValue::Audio => "Audio".to_string(),
        }
    }
}
