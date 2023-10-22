use handlebars::{handlebars_helper, JsonRender};
use internal_baml_parser_database::walkers::Walker;
use internal_baml_schema_ast::ast::{ClientId, WithDocumentation};
use log::info;
use serde_json::json;

use crate::generate::generate_python_client::file::clean_file_name;

use super::{
    file::File,
    template::render_template,
    traits::{JsonHelper, WithToCode, WithWritePythonString},
    FileCollector,
};

impl JsonHelper for Walker<'_, ClientId> {
    fn json(&self, f: &mut File) -> serde_json::Value {
        json!({
            "name": self.name(),
            "kwargs": {
                "provider": "openai"
            },
            "options": {},
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
