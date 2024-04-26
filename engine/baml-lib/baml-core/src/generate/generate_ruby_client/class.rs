use serde_json::json;

use super::field_type::{to_internal_type, to_internal_type_constructor, to_type_check};
use crate::generate::{
    dir_writer::WithFileContentRuby,
    ir::{Class, Expression, Walker},
};

use super::{
    ruby_language_features::{RubyLanguageFeatures, TSFileCollector, ToRuby},
    template::render_with_hbs,
};

impl WithFileContentRuby<RubyLanguageFeatures> for Walker<'_, &Class> {
    fn file_dir(&self) -> &'static str {
        "."
    }

    fn file_name(&self) -> String {
        "types".into()
    }

    fn write(&self, collector: &mut TSFileCollector) {
        todo!()
    }
}
