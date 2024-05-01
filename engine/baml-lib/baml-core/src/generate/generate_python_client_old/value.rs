use internal_baml_schema_ast::ast::{Expression, WithName};
use serde_json::Value;

#[allow(dead_code)]
pub(super) fn to_py_value(val: &Value) -> String {
    match val {
        Value::Null => "None".to_string(),
        Value::Bool(b) => match b {
            true => "True".to_string(),
            false => "False".to_string(),
        },
        Value::Number(n) => n.to_string(),
        Value::String(_) => val.to_string(),
        Value::Array(arr) => format!(
            "[{}]",
            arr.iter()
                .map(|v| to_py_value(v))
                .collect::<Vec<String>>()
                .join(", ")
        ),
        Value::Object(obj) => {
            let mut repr = "{".to_string();
            for (k, v) in obj {
                repr.push_str(&format!("\"{}\": {}, ", k, to_py_value(v)));
            }
            repr.push_str("}");
            repr
        }
    }
}

pub(super) fn expr_to_py_value(expr: &Expression) -> String {
    match expr {
        Expression::BoolValue(v, _) => {
            if *v {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        Expression::NumericValue(n, _) => n.to_string(),
        Expression::Identifier(idn) => {
            format!("\"{}\"", idn.name().to_string().replace('"', "\\\""))
        }
        Expression::StringValue(v, _) => format!("\"{}\"", v.replace('"', "\\\"")),
        Expression::RawStringValue(v) => {
            format!("\"\"\"{}\"\"\"", v.value().replace("\"\"\"", "\\\"\"\"\""))
        }
        Expression::Array(val, _) => {
            format!(
                "[{}]",
                val.iter()
                    .map(|v| expr_to_py_value(v))
                    .collect::<Vec<String>>()
                    .join(", ")
            )
        }
        Expression::Map(kv, _) => {
            let mut repr = "{".to_string();
            for (k, v) in kv {
                repr.push_str(&format!("\"{}\": {}, ", k, expr_to_py_value(v)));
            }
            repr.push_str("}");
            repr
        }
    }
}
