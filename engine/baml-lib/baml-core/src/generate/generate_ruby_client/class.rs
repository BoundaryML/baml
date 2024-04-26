use askama::Template;
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

#[derive(askama::Template)] // this will generate the code...
#[template(path = "class.rb.j2", escape = "none", print = "all")] // using the template in this path, relative
struct RubyStruct<'a> {
    name: &'a str,
    fields: Vec<(&'a str, String)>,
}

impl WithFileContentRuby<RubyLanguageFeatures> for Walker<'_, &Class> {
    fn file_dir(&self) -> &'static str {
        "."
    }

    fn file_name(&self) -> String {
        "types".into()
    }

    fn write(&self, collector: &mut TSFileCollector) {
        let file = collector.start_file(self.file_dir(), self.file_name(), false);
        file.append(
            RubyStruct {
                name: self.name(),
                fields: self
                    .item
                    .elem
                    .static_fields
                    .iter()
                    .map(|f| (f.elem.name.as_str(), f.elem.r#type.elem.to_ruby()))
                    .collect(),
            }
            .render()
            .unwrap_or("# Error rendering enum".to_string()),
        );
        file.add_export(&self.elem().name);
        collector.finish_file();
    }
}
