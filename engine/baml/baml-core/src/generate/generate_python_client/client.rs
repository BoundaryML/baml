use std::collections::HashMap;

use internal_baml_parser_database::walkers::{ClientWalker, Walker};
use internal_baml_schema_ast::ast::{ClientId, Expression, WithDocumentation};

use serde_json::{json, Value};

use crate::generate::generate_python_client::file::clean_file_name;

use super::{
    file::File,
    template::render_template,
    traits::{JsonHelper, WithWritePythonString},
    FileCollector,
};

pub fn expression_to_json(exp: &Expression) -> Value {
    match exp {
        Expression::NumericValue(val, _) => val
            .parse()
            .ok()
            .map(Value::Number)
            .unwrap_or_else(|| unreachable!("Error parsing numeric value")),
        Expression::StringValue(val, _) => Value::String(val.clone()),
        Expression::RawStringValue(val, _) => Value::String(val.clone()),
        Expression::Identifier(idn) => match idn {
            internal_baml_schema_ast::ast::Identifier::String(s, _) => Value::String(s.clone()),
            internal_baml_schema_ast::ast::Identifier::Invalid(s, _) => Value::String(s.clone()),
            internal_baml_schema_ast::ast::Identifier::Local(s, _) => Value::String(s.clone()),
            _ => Value::Null,
        },
        Expression::Array(arr, _) => {
            let json_arr: Vec<Value> = arr.iter().map(|x| expression_to_json(x)).collect();
            Value::Array(json_arr)
        }
        Expression::Map(map, _) => {
            let mut json_map = serde_json::Map::new();
            for (k, v) in map {
                let key = match expression_to_json(k) {
                    Value::String(s) => s,
                    _ => continue, // Skip if the key is not a string
                };
                let value = expression_to_json(v);
                json_map.insert(key, value);
            }
            Value::Object(json_map)
        }
    }
}

pub fn compute_map(expressions: &HashMap<String, Expression>) -> HashMap<String, Value> {
    let mut json_map = HashMap::new();
    for (key, value) in expressions {
        let computed_value = expression_to_json(value);
        json_map.insert(key.clone(), computed_value);
    }
    json_map
}

impl JsonHelper for ClientWalker<'_> {
    fn json(&self, _f: &mut File) -> serde_json::Value {
        let opts = compute_map(&self.properties().options);
        json!({
            "name": self.name(),
            "kwargs": {
                "provider": self.properties().provider
            },
            "options": opts,
            "doc_string": self.ast_client().documentation(),
        })
    }
}

impl WithWritePythonString for Walker<'_, ClientId> {
    fn file_name(&self) -> String {
        format!("client_{}", clean_file_name(self.name()))
    }

    fn write_py_file(&self, fc: &mut FileCollector) {
        fc.start_py_file("clients", "__init__");
        fc.complete_file();

        fc.start_py_file("clients", self.file_name());
        let json = self.json(fc.last_file());
        render_template(super::template::HSTemplate::Client, fc.last_file(), json);
        fc.complete_file();
    }
}
