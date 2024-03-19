use internal_baml_parser_database::{
    walkers::{EnumValueWalker, EnumWalker},
    ParserDatabase, WithStaticRenames,
};
use internal_baml_schema_ast::ast::{Enum, WithName};
use log::info;
use serde_json::json;

use super::{
    file::{File, FileCollector},
    template::render_template,
    traits::{JsonHelper, WithWritePythonString},
};
use crate::generate::{
    generate_python_client_old::file::clean_file_name,
    ir::repr::{Expression, NodeAttributes},
};

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
                    if let Some(Expression::String(alias)) = v.attributes.get("alias") {
                        if let Some(Expression::String(description)) =
                            v.attributes.get("description")
                        {
                            // "alias" and "alias: description"
                            return vec![
                                (format!("\"{}\"", alias.to_string()), &v.elem.0),
                                (format!("\"{}: {}\"", alias, description), &v.elem.0),
                            ];
                        }

                        return vec![(format!("\"{}\"", alias.to_string()), &v.elem.0)];
                    } else if let Some(Expression::String(description)) =
                        v.attributes.get("description")
                    {
                        // "description"
                        return vec![(format!("\"{}\"", description), &v.elem.0)];
                    }
                    vec![]
                })
                .map(|(alias, value_name)| format!("  {}: \"{}\"", alias, value_name))
                .collect::<Vec<_>>(),
            Err(e) => vec![], // TODO: handle error
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
