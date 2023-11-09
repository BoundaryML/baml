use internal_baml_parser_database::{
    walkers::{EnumValueWalker, EnumWalker},
    WithStaticRenames,
};
use internal_baml_schema_ast::ast::WithName;
use log::info;
use serde_json::json;

use crate::generate::generate_python_client::file::clean_file_name;

use super::{
    file::{File, FileCollector},
    template::render_template,
    traits::{JsonHelper, WithWritePythonString},
};

impl JsonHelper for EnumWalker<'_> {
    fn json(&self, _f: &mut File) -> serde_json::Value {
        json!({
            "name": self.name(),
            "values": self.values().map(|v| v.json(_f)).collect::<Vec<_>>(),
        })
    }
}

impl JsonHelper for EnumValueWalker<'_> {
    fn json(&self, _f: &mut File) -> serde_json::Value {
        json!({
            "name": self.name(),
            "alias": self.maybe_alias(self.db),
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
        info!("Writing enum: {}", self.name());
        info!("JSON: {}", serde_json::to_string_pretty(&json).unwrap());
        render_template(super::template::HSTemplate::Enum, fc.last_file(), json);
        fc.complete_file();
    }
}
