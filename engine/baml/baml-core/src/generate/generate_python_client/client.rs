use std::collections::HashMap;

use internal_baml_parser_database::walkers::{ClientWalker, Walker};
use internal_baml_schema_ast::ast::{
    ClientId, Expression, Identifier, WithDocumentation, WithName,
};

use serde_json::{json, Value};

use crate::generate::generate_python_client::file::clean_file_name;

use super::{
    file::File,
    template::render_template,
    traits::{JsonHelper, WithWritePythonString},
    FileCollector,
};

fn escaped_string(s: &str, quotes: (&'static str, &'static str)) -> String {
    s.replace("\\", "\\\\").replace(quotes.0, quotes.1)
}

impl JsonHelper for Identifier {
    fn json(&self, f: &mut File) -> serde_json::Value {
        match self {
            Identifier::ENV(s, _) => {
                f.add_import("os", "environ");
                Value::String(format!("environ['{}']", s.clone()))
            }
            _ => Value::String(format!(
                "\"{}\"",
                escaped_string(self.name(), ("\"", "\\\""))
            )),
        }
    }
}

impl JsonHelper for Expression {
    fn json(&self, f: &mut File) -> serde_json::Value {
        match self {
            Expression::NumericValue(val, _) => val
                .parse()
                .ok()
                .map(Value::Number)
                .unwrap_or_else(|| unreachable!("Error parsing numeric value")),
            Expression::StringValue(val, _) => {
                Value::String(format!("\"{}\"", escaped_string(val, ("\"", "\\\""))))
            }
            Expression::RawStringValue(val, _) => Value::String(format!(
                "\"\"\"\\\n{}\\\n\"\"\"",
                escaped_string(val, ("\"\"\"", "\\\"\\\"\\\""))
            )),
            Expression::Identifier(idn) => idn.json(f),
            Expression::Array(arr, _) => {
                let json_arr: Vec<Value> = arr.iter().map(|x| x.json(f)).collect();
                Value::Array(json_arr)
            }
            Expression::Map(map, _) => {
                let mut json_map = serde_json::Map::new();
                for (k, v) in map {
                    let key = match k.json(f) {
                        Value::String(s) => s,
                        _ => continue, // Skip if the key is not a string
                    };
                    let value = v.json(f);
                    json_map.insert(key, value);
                }
                Value::Object(json_map)
            }
        }
    }
}

impl JsonHelper for ClientWalker<'_> {
    fn json(&self, f: &mut File) -> serde_json::Value {
        let opts: HashMap<String, Value> = HashMap::from_iter(
            self.properties()
                .options
                .iter()
                .map(|(k, v)| (k.clone(), v.json(f))),
        );

        json!({
            "name": self.name(),
            "kwargs": {
                "provider": serde_json::to_string(&self.properties().provider).unwrap(),
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
