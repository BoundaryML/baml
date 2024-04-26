use serde_json::json;

use super::ruby_language_features::ToRuby;
use crate::generate::{
    dir_writer::WithFileContentRuby,
    ir::{repr, Function, FunctionArgs, Walker},
};

use super::{
    field_type::walk_custom_types,
    ruby_language_features::{RubyLanguageFeatures, TSFileCollector},
    template::render_with_hbs,
};

impl WithFileContentRuby<RubyLanguageFeatures> for Walker<'_, &Function> {
    fn file_dir(&self) -> &'static str {
        "."
    }

    fn file_name(&self) -> String {
        "function".into()
    }

    fn write(&self, collector: &mut TSFileCollector) {
        todo!()
    }
}
