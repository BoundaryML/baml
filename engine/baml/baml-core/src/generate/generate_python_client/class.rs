use internal_baml_parser_database::walkers::Walker;
use internal_baml_schema_ast::ast::{ClassId, FieldArity};
use serde_json::json;

use crate::generate::generate_python_client::file::clean_file_name;

use super::{
    file::{File, FileCollector},
    template::render_template,
    traits::{JsonHelper, WithToCode, WithWritePythonString},
};

impl WithWritePythonString for Walker<'_, ClassId> {
    fn file_name(&self) -> String {
        format!("cls_{}", clean_file_name(self.name()))
    }

    fn write_py_file(&self, fc: &mut FileCollector) {
        fc.start_py_file("types/classes", "__init__");
        fc.complete_file();
        fc.start_py_file("types", "__init__");
        fc.last_file()
            .add_import(&format!(".classes.{}", self.file_name()), self.name());
        fc.complete_file();

        fc.start_py_file("types/classes", self.file_name());
        let json = self.json(fc.last_file());
        render_template(super::template::HSTemplate::Class, fc.last_file(), json);
        fc.complete_file();
    }
}

impl JsonHelper for Walker<'_, ClassId> {
    fn json(&self, f: &mut File) -> serde_json::Value {
        json!({
            "name": self.name(),
            "fields": self.static_fields().map(|field|
                json!({
                "name": field.name(),
                "type": field.r#type().to_py_string(f),
                "optional": field.r#type().0 == FieldArity::Optional,
            })).collect::<Vec<_>>(),
        })
    }
}
