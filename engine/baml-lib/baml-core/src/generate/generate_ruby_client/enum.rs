use askama::Template;

use crate::generate::{
    dir_writer::WithFileContentRuby,
    ir::{Enum, Walker},
};

use super::{
    ruby_language_features::{RubyLanguageFeatures, TSFileCollector},
    template::render_with_hbs,
};

#[derive(Template)] // this will generate the code...
#[template(path = "enum.rb.j2", escape = "none")] // using the template in this path, relative
struct RubyEnum<'a> {
    name: &'a str,
    values: Vec<&'a str>,
}

impl WithFileContentRuby<RubyLanguageFeatures> for Walker<'_, &Enum> {
    fn file_dir(&self) -> &'static str {
        "."
    }

    fn file_name(&self) -> String {
        "types".into()
    }

    fn write(&self, collector: &mut TSFileCollector) {
        let file = collector.start_file(self.file_dir(), self.file_name(), false);
        file.append(
            RubyEnum {
                name: self.name(),
                values: self
                    .item
                    .elem
                    .values
                    .iter()
                    .map(|v| v.elem.0.as_str())
                    .collect(),
            }
            .render()
            .unwrap_or("# Error rendering enum".to_string()),
        );
        file.add_export(&self.elem().name);
        collector.finish_file();
    }
}
