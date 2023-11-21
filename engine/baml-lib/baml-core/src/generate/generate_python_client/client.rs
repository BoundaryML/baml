use internal_baml_parser_database::walkers::{ClientWalker, Walker};
use internal_baml_schema_ast::ast::{
    ClientId, Expression, Identifier, WithDocumentation, WithName,
};

use serde_json::json;

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

impl ToPyObject for Identifier {
    fn to_py_object(&self, f: &mut File) -> String {
        match self {
            Identifier::ENV(s, _) => {
                f.add_import("os", "environ");
                format!("environ['{}']", s.clone())
            }
            _ => format!("\"{}\"", escaped_string(self.name(), ("\"", "\\\""))),
        }
    }
}

impl ToPyObject for Expression {
    fn to_py_object(&self, f: &mut File) -> String {
        match self {
            Expression::NumericValue(val, _) => val.clone(),
            Expression::StringValue(val, _) => {
                format!("\"{}\"", escaped_string(val, ("\"", "\\\"")))
            }
            Expression::RawStringValue(val) => format!(
                "\"\"\"\\\n{}\\\n\"\"\"",
                escaped_string(val.value(), ("\"\"\"", "\\\"\\\"\\\""))
            ),
            Expression::Identifier(idn) => idn.to_py_object(f),
            Expression::Array(arr, _) => {
                let json_arr: Vec<_> = arr.iter().map(|x| x.to_py_object(f)).collect();
                format!("[{}]", json_arr.join(", "))
            }
            Expression::Map(map, _) => {
                let kvs = map
                    .iter()
                    .map(|(k, v)| {
                        let key = k.to_py_object(f);
                        let value = v.to_py_object(f);
                        (key, value)
                    })
                    .map(|(k, v)| format!("{}: {}", k, v))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{}}}", kvs)
            }
        }
    }
}

impl JsonHelper for ClientWalker<'_> {
    fn json(&self, f: &mut File) -> serde_json::Value {
        let props = self.properties();
        let opts = props
            .options
            .iter()
            .map(|(k, v)| {
                json!({
                            "key": k.clone(),
                            "value": v.to_py_object(f),
                })
            })
            .collect::<Vec<_>>();

        let retry_policy = props
            .retry_policy
            .as_ref()
            .map(|(policy, _)| {
                f.add_import(" ..configs.retry_policy", &policy);
                policy.as_str()
            })
            .unwrap_or("None");

        let redactions = props
            .options
            .iter()
            .filter_map(|(k, v)| {
                if v.is_env_expression() {
                    return Some(format!("\"{}\"", k));
                }
                None
            })
            .collect::<Vec<_>>()
            .join(", ");

        json!({
            "name": self.name(),
            "kwargs": {
                "provider": serde_json::to_string(&props.provider.0).unwrap(),
                "retry_policy": retry_policy,
                "redactions": format!("[{}]", redactions),
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

trait ToPyObject {
    fn to_py_object(&self, f: &mut File) -> String;
}
