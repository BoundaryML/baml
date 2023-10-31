use internal_baml_parser_database::walkers::{EnumWalker, FunctionWalker, VariantWalker};
use internal_baml_schema_ast::ast::{FunctionId, TopId, WithName};

use log::info;
use serde_json::json;

use crate::generate::generate_python_client::file::clean_file_name;

use super::{
    file::File,
    template::render_template,
    traits::{JsonHelper, WithWritePythonString},
    FileCollector,
};

impl<'db> JsonHelper for VariantWalker<'db> {
    fn json(&self, f: &mut File) -> serde_json::Value {
        let func = self.walk_function().unwrap();
        let client = self.client().unwrap();
        f.add_import(
            &format!(".{}", func.file_name()),
            &format!("BAML{}", func.name()),
        );
        f.add_import(&format!("..clients.{}", client.file_name()), client.name());

        let mut prompt = self.properties().prompt.value.clone();

        let (input, output) = &self.properties().replacers;

        input.iter().for_each(|(k, val)| {
            prompt = prompt.replace(&k.key(), &format!("{{{}}}", val));
        });

        json!({
            "name": self.identifier().name(),
            "function": func.json(f),
            "prompt": prompt,
            "client": client.name(),
            "output_replacers": output.iter().map(|(var, replacement)| json!({
                "key": var.key(),
                "value": replacement
            })).collect::<Vec<_>>(),
        })
    }
}

impl WithWritePythonString for VariantWalker<'_> {
    fn file_name(&self) -> String {
        format!(
            "fx_{}_impl_{}",
            clean_file_name(self.function_identifier().name()),
            clean_file_name(self.identifier().name())
        )
    }

    fn write_py_file(&self, fc: &mut FileCollector) {
        fc.start_py_file("functions", self.file_name());
        let json = self.json(fc.last_file());
        info!("Writing variant: {}", self.identifier().name());
        info!("JSON: {}", &json);
        render_template(super::template::HSTemplate::Variant, fc.last_file(), json);
        fc.complete_file();
    }
}
