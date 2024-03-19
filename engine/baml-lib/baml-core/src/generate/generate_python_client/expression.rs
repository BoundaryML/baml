use crate::generate::ir::{Expression, Identifier, TypeValue};

use super::python_language_features::ToPython;

fn escaped_string(s: &str, quotes: (&'static str, &'static str)) -> String {
    s.replace("\\", "\\\\").replace(quotes.0, quotes.1)
}

impl ToPython for Expression {
    fn to_py(&self) -> String {
        print!("expression {:#?}", self);
        match self {
            Expression::List(values) => {
                format!(
                    "[{}]",
                    values
                        .iter()
                        .map(|v| v.to_py())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Expression::Map(values) => {
                let kvs = values
                    .iter()
                    .map(|(k, v)| {
                        let key = k.to_py();
                        let value = v.to_py();
                        (key, value)
                    })
                    .map(|(k, v)| format!("{}: {}", k, v))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{}}}", kvs)
            }
            Expression::Identifier(idn) => match idn {
                Identifier::ENV(idn) => format!("os.environ['{}']", idn),
                // a string with no quotes for example
                Identifier::Local(k) => {
                    format!("\"{}\"", escaped_string(k, ("\"", "\\\"")))
                }
                Identifier::Ref(r) => format!("\"{}\"", r.join(".")),
                Identifier::Primitive(p) => p.to_py(),
            },
            Expression::String(val) => format!("\"{}\"", escaped_string(val, ("\"", "\\\""))),
            Expression::RawString(val) => format!(
                "\"\"\"\\\n{}\\\n\"\"\"",
                escaped_string(val, ("\"\"\"", "\\\"\\\"\\\""))
            ),
            Expression::Numeric(val) => val.clone(),
        }
    }
}

impl ToPython for TypeValue {
    fn to_py(&self) -> String {
        match self {
            TypeValue::Bool => "bool".to_string(),
            TypeValue::Float => "float".to_string(),
            TypeValue::Int => "int".to_string(),
            TypeValue::String => "str".to_string(),
            TypeValue::Null => "None".to_string(),
            TypeValue::Char => "str".to_string(),
        }
    }
}
