use internal_baml_parser_database::walkers::Walker;
use internal_baml_schema_ast::ast::EnumId;
use serde_json::json;

use super::{
    file::{File, FileCollector},
    template::render_template,
    traits::{JsonHelper, WithWritePythonString},
};

impl JsonHelper for Walker<'_, EnumId> {
    fn json(&self, _f: &mut File) -> serde_json::Value {
        json!({
            "name": self.name(),
            "values": self.values().map(|v| v.name()).collect::<Vec<_>>(),
        })
    }
}

impl WithWritePythonString for Walker<'_, EnumId> {
    fn write_py_file<'a>(&'a self, fc: &'a mut FileCollector) {
        fc.start_py_file("types/enums", self.name());
        let json = self.json(fc.last_file());
        render_template(super::template::HSTemplate::Enum, fc.last_file(), json);
        fc.complete_file();
    }
}
