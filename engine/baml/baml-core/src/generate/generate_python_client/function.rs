use handlebars::{handlebars_helper, JsonRender};
use internal_baml_parser_database::walkers::Walker;
use internal_baml_schema_ast::ast::{
    ClassId, FieldArity, FieldId, FieldType, FunctionArg, FunctionArgs, FunctionId, Identifier,
    NamedFunctionArgList, TypeValue, WithDocumentation, WithName,
};
use log::info;
use serde_json::json;

use super::{
    file::File,
    template::render_template,
    traits::{JsonHelper, WithToCode, WithWritePythonString},
    FileCollector,
};

impl JsonHelper for FunctionArgs {
    fn json(&self, f: &mut File) -> serde_json::Value {
        match self {
            FunctionArgs::Named(arg_list) => json!({
                "named_args": arg_list.args.iter().map(|(id, arg)| {
                    json!({
                        "name": id.to_py_string(f),
                        "type": arg.to_py_string(f),
                    })
                }).collect::<Vec<_>>(),
            }),
            FunctionArgs::Unnamed(arg) => json!({
                "unnamed_arg": json!({
                    "type": arg.to_py_string(f),
                })
            }),
        }
    }
}

impl JsonHelper for Walker<'_, FunctionId> {
    fn json(&self, f: &mut File) -> serde_json::Value {
        json!({
            "name": self.ast_function().name(),
            "args": self.ast_function().input().json(f),
            "return": self.ast_function().output().json(f),
            "doc_string": self.ast_function().documentation(),
            "impls": vec!["impl0", "impl1"],
        })
    }
}

impl WithWritePythonString for Walker<'_, FunctionId> {
    fn write_py_file(&self, fc: &mut FileCollector) {
        fc.start_py_file("functions", self.name());
        let json = self.json(fc.last_file());
        render_template(super::template::HSTemplate::Function, fc.last_file(), json);
        fc.complete_file();

        fc.start_py_file("functions", format!("{}.pyi", self.name()));
        let json = self.json(fc.last_file());
        render_template(
            super::template::HSTemplate::FunctionPYI,
            fc.last_file(),
            json,
        );
        fc.complete_file();
    }
}
