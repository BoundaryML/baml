use internal_baml_parser_database::{
    walkers::{ClassWalker, FieldWalker},
    WithStaticRenames,
};
use internal_baml_schema_ast::ast::WithName;
use serde_json::json;

use crate::generate::generate_python_client::file::clean_file_name;

use super::{
    file::{File, FileCollector},
    template::render_template,
    traits::{JsonHelper, WithToCode, WithWritePythonString},
};

impl WithWritePythonString for ClassWalker<'_> {
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

        self.required_classes().for_each(|f| {
            fc.last_file()
                .add_import(&format!(".{}", f.file_name()), f.name());
        });
        self.required_enums().for_each(|f| {
            fc.last_file()
                .add_import(&format!("..enums.{}", f.file_name()), f.name());
        });
        let json = self.json(fc.last_file());
        render_template(super::template::HSTemplate::Class, fc.last_file(), json);
        fc.complete_file();
    }
}

impl JsonHelper for ClassWalker<'_> {
    fn json(&self, f: &mut File) -> serde_json::Value {
        json!({
            "name": self.name(),
            "fields": self.static_fields().map(|field|
                field.json(f)
                ).collect::<Vec<_>>(),
            "properties": self.dynamic_fields().map(|field|
                field.json(f)
            ).collect::<Vec<_>>(),
            "num_fields": self.ast_class().fields().len(),
        })
    }
}

impl JsonHelper for FieldWalker<'_> {
    fn json(&self, f: &mut File) -> serde_json::Value {
        //log::info!("FieldWalker::json {} {}", self.name(), self.is_dynamic());
        match self.is_dynamic() {
            true => json!({
                "name": self.name(),
                "type": self.r#type().to_py_string(f),
                "code": self.code_for_language("python").unwrap_or("raise NotImplementedError()"),
            }),
            false => json!({
                "name": self.name(),
                "type": self.r#type().to_py_string(f),
                "optional": self.r#type().is_nullable(),
                "alias": self.maybe_alias(self.db),
            }),
        }
    }
}
