use handlebars::{handlebars_helper, JsonRender};
use internal_baml_parser_database::walkers::{ArgWalker, FunctionImplWalker, Walker};
use internal_baml_schema_ast::ast::{
    ClassId, FieldArity, FieldId, FieldType, FunctionArg, FunctionArgs, FunctionId, Identifier,
    NamedFunctionArgList, TypeValue, WithDocumentation, WithName,
};
use log::info;
use serde_json::json;

use crate::generate::generate_python_client::file::clean_file_name;

use super::{
    file::File,
    template::render_template,
    traits::{JsonHelper, WithWritePythonString},
    FileCollector,
};

impl JsonHelper for FunctionImplWalker<'_> {
    fn json(&self, f: &mut File) -> serde_json::Value {
        let func = self.walk_function();
        let client = self.client();
        f.add_import(
            &format!(".{}", func.file_name()),
            &format!("BAML{}", func.name()),
        );
        f.add_import(&format!("..clients.{}", client.file_name()), client.name());
        json!({
            "name": self.name(),
            "function": func.json(f),
            "prompt": self.ast_variant().prompt().unwrap_or("This is a default prompt"),
            "client": client.name(),
        })
    }
}

impl WithWritePythonString for FunctionImplWalker<'_> {
    fn file_name(&self) -> String {
        format!(
            "fx_{}_impl_{}",
            clean_file_name(self.function_name()),
            clean_file_name(self.name())
        )
    }

    fn write_py_file(&self, fc: &mut FileCollector) {
        fc.start_py_file("functions", self.file_name());
        let json = self.json(fc.last_file());
        render_template(super::template::HSTemplate::Variant, fc.last_file(), json);
        fc.complete_file();
    }
}
