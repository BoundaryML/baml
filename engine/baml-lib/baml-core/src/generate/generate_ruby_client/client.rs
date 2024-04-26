use indexmap::IndexMap;
use serde_json::json;

use crate::generate::{
    dir_writer::WithFileContentRuby,
    ir::{Client, Walker},
};

use super::{
    ruby_language_features::{RubyLanguageFeatures, TSFileCollector, ToRuby},
    template::render_with_hbs,
};

impl WithFileContentRuby<RubyLanguageFeatures> for Walker<'_, &Client> {
    fn file_dir(&self) -> &'static str {
        "."
    }

    fn file_name(&self) -> String {
        "client".into()
    }

    fn write(&self, collector: &mut TSFileCollector) {
        todo!()
    }
}
