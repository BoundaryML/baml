use serde_json::json;

use super::{python_language_features::PythonLanguageFeatures, template::render_with_hbs};

use crate::generate::{dir_writer::WithFileContentPy as WithFileContent, ir::IntermediateRepr};

impl WithFileContent<PythonLanguageFeatures> for IntermediateRepr {
    fn file_dir(&self) -> &'static str {
        "./"
    }

    fn file_name(&self) -> String {
        "index".into()
    }

    fn write(&self, fc: &mut crate::generate::dir_writer::FileCollector<PythonLanguageFeatures>) {
        let file = fc.start_file(self.file_dir(), self.file_name(), false);
        file.append(render_with_hbs(
            super::template::Template::ExportFile,
            &json!({
              "functions": self.walk_functions().map(|f| f.elem().name.clone()).collect::<Vec<_>>(),
            }),
        ));
        fc.finish_file();
    }
}
