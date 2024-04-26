use serde_json::json;

use super::ruby_language_features::ToRuby;
use crate::generate::{
    dir_writer::WithFileContentRuby,
    ir::{self, Function, TestCase, Walker},
};

use super::{
    ruby_language_features::{RubyLanguageFeatures, TSFileCollector},
    template::render_with_hbs,
};

impl WithFileContentRuby<RubyLanguageFeatures> for Walker<'_, (&Function, &TestCase)> {
    fn file_dir(&self) -> &'static str {
        "./__tests__"
    }

    fn file_name(&self) -> String {
        format!("{}.test", self.item.0.elem.name())
    }

    fn write(&self, collector: &mut TSFileCollector) {
        todo!()
    }
}
