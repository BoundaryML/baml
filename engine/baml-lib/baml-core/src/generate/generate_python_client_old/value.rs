use serde_json::Value;

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
                repr.push_str(&format!("{}: {}, ", k, to_py_value(v)));
            }
            repr.push_str("}");
            repr
        }
    }
}
