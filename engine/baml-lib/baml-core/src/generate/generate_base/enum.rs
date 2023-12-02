use internal_baml_parser_database::{
    walkers::{EnumValueWalker, EnumWalker},
    WithStaticRenames,
};
use internal_baml_schema_ast::ast::WithName;
use serde_json::json;

use super::{
    file::{clean_file_name, File, FileCollector},
    template::render_template,
    traits::{JsonHelper, TargetLanguage, WithFileName},
};

impl JsonHelper for EnumWalker<'_> {
    fn json(&self, f: &mut File, lang: TargetLanguage) -> serde_json::Value {
        json!({
            "name": self.name(),
            "values": self.values().map(|v| v.json(f, lang)).collect::<Vec<_>>(),
        })
    }
}

impl JsonHelper for EnumValueWalker<'_> {
    fn json(&self, _f: &mut File, lang: TargetLanguage) -> serde_json::Value {
        json!({
            "name": self.name(),
            "alias": self.maybe_alias(self.db),
        })
    }
}

impl WithFileName for EnumWalker<'_> {
    fn file_name(&self) -> String {
        format!("enm_{}", clean_file_name(self.name()))
    }

    fn to_py_file<'a>(&'a self, fc: &'a mut FileCollector) {
        fc.start_py_file("types/enums", "__init__");
        fc.complete_file();
        fc.start_py_file("types", "__init__");
        fc.last_file()
            .add_import(&format!(".enums.{}", self.file_name()), self.name());
        fc.complete_file();

        fc.start_py_file("types/enums", self.file_name());
        let json = self.json(fc.last_file(), TargetLanguage::Python);
        render_template(
            TargetLanguage::Python,
            super::template::HSTemplate::Enum,
            fc.last_file(),
            json,
        );
        fc.complete_file();
    }

    fn to_ts_file(&self, fc: &mut FileCollector) {
        todo!()
    }
}
