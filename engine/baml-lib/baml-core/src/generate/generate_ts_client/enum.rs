use crate::generate::{dir_writer::WithFileContent, ir::Enum};

use super::{
    template::render_with_hbs,
    ts_language_features::{TSFileCollector, TSLanguageFeatures},
};

impl WithFileContent<TSLanguageFeatures> for Enum {
    fn file_dir(&self) -> &'static str {
        "."
    }

    fn file_name(&self) -> String {
        "types".into()
    }

    fn write(&self, collector: &mut TSFileCollector) {
        let file = collector.start_file(self.file_dir(), self.file_name(), false);
        file.append(render_with_hbs(super::template::Template::Enum, &self));
        file.add_export(self.elem.name.clone());
        collector.finish_file();

        let file = collector.start_file(self.file_dir(), self.file_name() + "_internal", false);
        file.add_import("./types", self.elem.name.clone(), None, false);
        file.append(render_with_hbs(
            super::template::Template::EnumInternal,
            &self,
        ));
        collector.finish_file();
    }
}
