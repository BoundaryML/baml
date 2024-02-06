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
    traits::{JsonHelper, WithPartial, WithToCode, WithWritePythonString},
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

        // Do the same thing, but now write the "Partial" types, which have all fields optional.
        fc.start_py_file("types/partial/classes", "__init__");
        fc.complete_file();
        fc.start_py_file("types/partial", "__init__");
        fc.last_file().add_import(
            &format!(".classes.{}", self.file_name()),
            &format!("Partial{}", self.name()),
        );
        fc.complete_file();

        fc.start_py_file("types/partial/classes", self.file_name());

        self.required_classes().for_each(|f| {
            fc.last_file().add_import(
                &format!("...classes.{}", f.file_name()),
                &format!("{}", f.name()),
            );
        });
        // Still import the regular enums, as the partials are not different
        self.required_enums().for_each(|f| {
            fc.last_file()
                .add_import(&format!("...enums.{}", f.file_name()), f.name());
        });

        let json = self.json(fc.last_file());
        render_template(
            super::template::HSTemplate::ClassPartial,
            fc.last_file(),
            json,
        );
        fc.complete_file();
    }
}

impl JsonHelper for ClassWalker<'_> {
    fn json(&self, f: &mut File) -> serde_json::Value {
        json!({
            "name": self.name(),
            "name_partial": "Partial".to_string() + &self.name(),
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
                "type_partial": self.r#type().to_partial_py_string(f),
                "code": self.code_for_language("python").unwrap_or("raise NotImplementedError()"),
            }),
            false => json!({
                "name": self.name(),
                "type": self.r#type().to_py_string(f),
                "type_partial": self.r#type().to_partial_py_string(f),
                "optional": self.r#type().is_nullable(),
                "can_be_null": self.r#type().can_be_null(),
                "alias": self.maybe_alias(self.db),
            }),
        }
    }
}
