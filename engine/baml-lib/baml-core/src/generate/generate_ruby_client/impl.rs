use serde_json::json;

use super::{field_type::to_parse_expression, ruby_language_features::ToRuby};
use crate::generate::{
    dir_writer::WithFileContentRuby,
    ir::{Function, FunctionArgs, Impl, Prompt, Walker},
};

use super::{
    ruby_language_features::{RubyLanguageFeatures, TSFileCollector},
    template::render_with_hbs,
};

impl WithFileContentRuby<RubyLanguageFeatures> for Walker<'_, (&Function, &Impl)> {
    fn file_dir(&self) -> &'static str {
        "./impls"
    }

    fn file_name(&self) -> String {
        format!("{}_{}", self.item.0.elem.name(), self.elem().name).to_lowercase()
    }

    fn write(&self, collector: &mut TSFileCollector) {
        todo!()
    }
}
