use internal_baml_parser_database::walkers::{ArgWalker, Walker};
use internal_baml_schema_ast::ast::{FunctionId, WithDocumentation, WithName};

use serde_json::json;

use super::{
    file::{clean_file_name, File, FileCollector},
    template::{render_template, HSTemplate},
    traits::{JsonHelper, TargetLanguage, WithFileName, WithToCode},
};

impl JsonHelper for ArgWalker<'_> {
    fn json(&self, f: &mut File, lang: TargetLanguage) -> serde_json::Value {
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
                "name": idn.to_code(f, lang),
                "type": arg.to_code(f, lang),
            }),
            (None, arg) => json!({
                "type": arg.to_code(f, lang),
            }),
        }
    }
}

impl JsonHelper for Walker<'_, FunctionId> {
    fn json(&self, f: &mut File, lang: TargetLanguage) -> serde_json::Value {
        let impls = self
            .walk_variants()
            .map(|v| v.name().to_string())
            .collect::<Vec<_>>();
        json!({
            "name": self.ast_function().name(),
            "unnamed_args": self.is_positional_args(),
            "args": self.walk_input_args().map(|a| a.json(f, lang)).collect::<Vec<_>>(),
            "return": self.walk_output_args().map(|a| a.json(f, lang)).collect::<Vec<_>>(),
            "doc_string": self.ast_function().documentation(),
            "impls": impls,
            "has_impls": impls.len() > 0,
            "default_impl": self.metadata().default_impl.as_ref().map(|f| f.0.clone())
        })
    }
}

impl WithFileName for Walker<'_, FunctionId> {
    fn file_name(&self) -> String {
        format!("fx_{}", clean_file_name(self.name()))
    }

    fn to_py_file(&self, fc: &mut FileCollector) {
        let lang = TargetLanguage::Python;
        fc.start_py_file("functions", "__init__");
        fc.complete_file();

        fc.start_py_file("functions", self.file_name());
        let json = self.json(fc.last_file(), lang);
        render_template(lang, HSTemplate::Function, fc.last_file(), json);
        fc.complete_file();

        fc.start_py_file("functions", format!("{}.pyi", self.file_name()));
        let json = self.json(fc.last_file(), lang);
        render_template(lang, HSTemplate::FunctionPYI, fc.last_file(), json);
        fc.complete_file();
    }

    fn to_ts_file(&self, fc: &mut FileCollector) {
        let lang = TargetLanguage::TypeScript;

        fc.start_ts_file("functions", self.file_name());
        let json = self.json(fc.last_file(), lang);
        render_template(lang, HSTemplate::Function, fc.last_file(), json);
        fc.complete_file();
    }
}
