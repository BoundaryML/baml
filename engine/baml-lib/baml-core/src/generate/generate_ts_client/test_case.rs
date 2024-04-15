use serde_json::json;

use crate::generate::{
    dir_writer::WithFileContent,
    generate_ts_client::{field_type::to_parse_expression, ts_language_features::ToTypeScript},
    ir::{self, Function, FunctionArgs, TestCase, Walker},
};

use super::{
    template::render_with_hbs,
    ts_language_features::{TSFileCollector, TSLanguageFeatures},
};

impl WithFileContent<TSLanguageFeatures> for Walker<'_, (&Function, &TestCase)> {
    fn file_dir(&self) -> &'static str {
        "./__tests__"
    }

    fn file_name(&self) -> String {
        format!("{}.test", self.item.0.elem.name().clone())
    }

    fn write(&self, collector: &mut TSFileCollector) {
        let (function, test_case) = self.item;

        let impls = match &function.elem {
            ir::repr::Function::V1(f) => f.impls.iter().map(|i| &i.elem.name).collect::<Vec<_>>(),
            ir::repr::Function::V2(f) => f.configs.iter().map(|c| &c.name).collect::<Vec<_>>(),
        };

        if impls.is_empty() {
            return;
        }

        let file = collector.start_file(self.file_dir(), self.file_name(), false);
        file.add_import_lib("../", Some("b"));
        file.add_import(
            "@boundaryml/baml-core/ffi_layer",
            "FireBamlEvent",
            None,
            false,
        );
        file.add_import("@boundaryml/baml-core/ffi_layer", "traceAsync", None, false);
        let test_content = json!({
          "function_name": function.elem.name().clone(),
          "test_name": test_case.elem.name.clone(),
          "impl_names": impls,
          "test_content": test_case.elem.content.to_ts(),
        });

        file.append(render_with_hbs(
            super::template::Template::TestCase,
            &test_content,
        ));
        collector.finish_file();
    }
}
