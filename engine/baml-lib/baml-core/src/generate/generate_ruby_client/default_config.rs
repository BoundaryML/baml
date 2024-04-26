use serde_json::json;

use super::{field_type::to_parse_expression, ruby_language_features::ToRuby};
use crate::generate::{
    dir_writer::WithFileContentRuby,
    ir::{repr::FunctionConfig, Function, FunctionArgs, Walker},
};

use super::{
    ruby_language_features::{RubyLanguageFeatures, TSFileCollector},
    template::render_with_hbs,
};

impl WithFileContentRuby<RubyLanguageFeatures> for Walker<'_, (&Function, &FunctionConfig)> {
    fn file_dir(&self) -> &'static str {
        "./impls"
    }

    fn file_name(&self) -> String {
        format!("{}_{}", self.item.0.elem.name(), self.item.1.name).to_lowercase()
    }

    fn write(&self, collector: &mut TSFileCollector) {
        todo!()
    }
}
