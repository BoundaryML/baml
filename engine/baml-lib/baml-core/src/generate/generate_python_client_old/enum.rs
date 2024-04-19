use internal_baml_parser_database::walkers::EnumWalker;
use internal_baml_schema_ast::ast::WithName;
use serde_json::json;

use super::{
    file::{File, FileCollector},
    template::render_template,
    traits::{JsonHelper, WithWritePythonString},
};
use crate::generate::{generate_python_client_old::file::clean_file_name, ir::repr::Expression};

use crate::generate::ir::repr::WithRepr;

impl JsonHelper for EnumWalker<'_> {
    fn json(&self, _f: &mut File) -> serde_json::Value {
        // let enum_values = self.values().map(|v| v.node(self.db)).collect::<Vec<_>>();
        let node = self.node(self.db);
        let alias_value_name_pairs = match node {
            Ok(e) => e
                .elem
                .values
                .iter()
                .flat_map(|v| -> Vec<(String, &String)> {
                    let alias = v.attributes.get("alias");
                    let description = v.attributes.get("description");
                    match (alias, description) {
                        (
                            Some(Expression::String(alias)),
                            Some(Expression::String(description)),
                        ) => {
                            // "alias" and "alias: description"
                            vec![
                                (format!("{}", alias.to_string()), &v.elem.0),
                                (format!("{}: {}", alias, description), &v.elem.0),
                            ]
                        }
                        (Some(Expression::String(alias)), None) => {
                            // "alias"
                            vec![(format!("{}", alias.to_string()), &v.elem.0)]
                        }
                        (None, Some(Expression::String(description))) => {
                            // "description"
                            vec![
                                (format!("{}: {}", v.elem.0, description), &v.elem.0),
                                (format!("{}", description), &v.elem.0),
                            ]
                        }
                        _ => vec![],
                    }
                })
                .map(|(alias, value_name)| {
                    format!(
                        "  \"{}\": \"{}\"",
                        alias.replace("\n", "\\n").replace("\"", "\\\""),
                        value_name
                    )
                })
                .collect::<Vec<_>>(),
            Err(_e) => vec![], // TODO: handle error
        };

        json!({
            "name": self.name(),
            "alias_pairs": alias_value_name_pairs.join(",\n"),
            "values": self.values().flat_map(|v|
                vec![
                    json!({
                        "name": v.name(),
                    }),
                ]).collect::<Vec<_>>(),
        })
    }
}

impl WithWritePythonString for EnumWalker<'_> {
    fn file_name(&self) -> String {
        format!("enm_{}", clean_file_name(self.name()))
    }

    fn write_py_file<'a>(&'a self, fc: &'a mut FileCollector) {
        fc.start_py_file("types/enums", "__init__");
        fc.complete_file();
        fc.start_py_file("types", "__init__");
        fc.last_file()
            .add_import(&format!(".enums.{}", self.file_name()), self.name());
        fc.complete_file();

        fc.start_py_file("types/enums", self.file_name());
        let json = self.json(fc.last_file());
        render_template(super::template::HSTemplate::Enum, fc.last_file(), json);
        fc.complete_file();
    }
}
