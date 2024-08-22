use baml_types::{BamlMediaType, TypeValue};
use internal_baml_core::ir::{Expression, Identifier};

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
            Expression::JinjaExpression(_expr) => panic!("TODO"),
        }
    }
}

impl ToRuby for TypeValue {
    fn to_ruby(&self) -> String {
        match self {
            TypeValue::Bool => "boolean",
            TypeValue::Float => "number",
            TypeValue::Int => "number",
            TypeValue::String => "string",
            TypeValue::Null => "null",
            TypeValue::Media(BamlMediaType::Image) => "Image",
            TypeValue::Media(BamlMediaType::Audio) => "Audio",
        }
        .to_string()
    }
}
