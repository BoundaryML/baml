use indexmap::IndexMap;
use serde_json::json;

use crate::generate::{
    dir_writer::WithFileContent,
    ir::{Client, Walker},
};

use super::{
    template::render_with_hbs,
    ts_language_features::{TSFileCollector, TSLanguageFeatures, ToTypeScript},
};

impl WithFileContent<TSLanguageFeatures> for Walker<'_, &Client> {
    fn file_dir(&self) -> &'static str {
        "."
    }

    fn file_name(&self) -> String {
        "client".into()
    }

    fn write(&self, collector: &mut TSFileCollector) {
        let file = collector.start_file(self.file_dir(), self.file_name(), false);
        file.add_import(
            "@boundaryml/baml-client/client_manager",
            "clientManager",
            None,
            false,
        );
        file.append(render_with_hbs(
            super::template::Template::Client,
            &json!({
                "name": self.elem().name,
                "provider": self.elem().provider,
                "options": self.elem().options.iter().map(|(k, v)| (k.clone(), v.to_ts())).collect::<IndexMap<_, _>>(),
            }),
        ));
        file.add_export(self.elem().name.clone());
        collector.finish_file();
    }
}
