use crate::generate::{
    dir_writer::WithFileContentRuby,
    ir::{Enum, Walker},
};

use super::{
    ruby_language_features::{RubyLanguageFeatures, TSFileCollector},
    template::render_with_hbs,
};

impl WithFileContentRuby<RubyLanguageFeatures> for Walker<'_, &Enum> {
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
