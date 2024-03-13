use serde_json::json;

use crate::generate::{
    dir_writer::WithFileContentPy as WithFileContent,
    ir::{Enum, Walker},
};

use super::{
    python_language_features::{PythonFileCollector, PythonLanguageFeatures, ToPython},
    template::render_with_hbs,
};

impl WithFileContent<PythonLanguageFeatures> for Walker<'_, &Enum> {
    fn file_dir(&self) -> &'static str {
        "."
    }

    fn file_name(&self) -> String {
        "types".into()
    }

    fn write(&self, collector: &mut PythonFileCollector) {
        let file = collector.start_file(self.file_dir(), self.file_name(), false);
        file.append(render_with_hbs(
            super::template::Template::Enum,
            &json!({
              "name": self.elem().name,
              "values": self.elem().values.iter().flat_map(|v| vec!{json!({
                "alias": v.attributes.get("alias").map(|s| s.to_py()),
                "name": v.elem,
              }),
              json!({
                "alias": format!("{}: {}", v.attributes.get("alias").map(|s| s.to_py()), v.attributes.get("description").map(|s| s.to_py())),
                "name": v.elem,

              })
            }).collect::<Vec<_>>()
            }),
        ));
        file.add_export(&self.elem().name);
        collector.finish_file();

        let file = collector.start_file(self.file_dir(), self.file_name() + "_internal", false);
        file.add_import("./types", self.elem().name.clone(), None, false);
        // file.append(render_with_hbs(
        //     super::template::Template::EnumInternal,
        //     &self.item,
        // ));
        collector.finish_file();
    }
}
