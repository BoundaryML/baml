use serde_json::json;

use crate::generate::{dir_writer::WithFileContentRuby, ir::IntermediateRepr};

use super::{ruby_language_features::RubyLanguageFeatures, template::render_with_hbs};

impl WithFileContentRuby<RubyLanguageFeatures> for IntermediateRepr {
    fn file_dir(&self) -> &'static str {
        "./"
    }

    fn file_name(&self) -> String {
        "index".into()
    }

    fn write(&self, fc: &mut crate::generate::dir_writer::FileCollector<RubyLanguageFeatures>) {
        todo!()
    }
}
