use internal_baml_parser_database::walkers::{ArgWalker, Walker};
use internal_baml_schema_ast::ast::{FunctionId, WithDocumentation, WithName};

use serde_json::json;

use crate::generate::generate_python_client::file::clean_file_name;

use super::{
    file::File,
    template::render_template,
    traits::{JsonHelper, WithToCode, WithWritePythonString},
    FileCollector,
};

impl JsonHelper for ArgWalker<'_> {
    fn json(&self, f: &mut File) -> serde_json::Value {
        let _ = self
            .required_classes()
            .map(|cls| f.add_import(&format!("..types.classes.{}", cls.file_name()), cls.name()))
            .collect::<Vec<_>>();
        let _ = self
            .required_enums()
            .map(|enm| f.add_import(&format!("..types.enums.{}", enm.file_name()), enm.name()))
            .collect::<Vec<_>>();

        match self.ast_arg() {
            (Some(idn), arg) => json!({
                "name": idn.to_py_string(f),
                "type": arg.to_py_string(f),
            }),
            (None, arg) => json!({
                "type": arg.to_py_string(f),
            }),
        }
    }
}

impl JsonHelper for Walker<'_, FunctionId> {
    fn json(&self, f: &mut File) -> serde_json::Value {
        let impls = self
            .walk_variants()
            .map(|v| v.name().to_string())
            .collect::<Vec<_>>();
        json!({
            "name": self.ast_function().name(),
            "unnamed_args": self.is_positional_args(),
            "args": self.walk_input_args().map(|a| a.json(f)).collect::<Vec<_>>(),
            "return": self.walk_output_args().map(|a| a.json(f)).collect::<Vec<_>>(),
            "doc_string": self.ast_function().documentation(),
            "impls": impls,
            "has_impls": impls.len() > 0,
        })
    }
}

impl WithWritePythonString for Walker<'_, FunctionId> {
    fn file_name(&self) -> String {
        format!("fx_{}", clean_file_name(self.name()))
    }

    fn write_py_file(&self, fc: &mut FileCollector) {
        fc.start_py_file("functions", "__init__");
        fc.complete_file();

        fc.start_py_file("functions", self.file_name());
        let json = self.json(fc.last_file());
        render_template(super::template::HSTemplate::Function, fc.last_file(), json);
        fc.complete_file();

        fc.start_py_file("functions", format!("{}.pyi", self.file_name()));
        let json = self.json(fc.last_file());
        render_template(
            super::template::HSTemplate::FunctionPYI,
            fc.last_file(),
            json,
        );
        fc.complete_file();
    }
}
