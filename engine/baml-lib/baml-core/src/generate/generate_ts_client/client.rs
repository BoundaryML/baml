use indexmap::IndexMap;
use serde_json::json;

use crate::generate::{
    dir_writer::WithFileContentTs,
    ir::{Client, Walker},
};

use super::{
    template::render_with_hbs,
    ts_language_features::{TSFileCollector, TSLanguageFeatures, ToTypeScript},
};

impl WithFileContentTs<TSLanguageFeatures> for Walker<'_, &Client> {
    fn file_dir(&self) -> &'static str {
        "."
    }

    fn file_name(&self) -> String {
        "client".into()
    }

    fn write(&self, collector: &mut TSFileCollector) {
        let file = collector.start_file(self.file_dir(), self.file_name(), false);
        if let Some(retry_policy_id) = self.elem().retry_policy_id.as_ref() {
            file.add_import("./retry_policy", retry_policy_id, None, false);
        }
        file.add_import(
            "@boundaryml/baml-core/client_manager",
            "clientManager",
            None,
            false,
        );
        let mut options = self
            .elem()
            .options
            .iter()
            .map(|(k, v)| (k.clone(), v.to_ts()))
            .collect::<IndexMap<_, _>>();
        if let Some(retry_policy_id) = self.elem().retry_policy_id.as_ref() {
            options.insert("retry_policy".into(), retry_policy_id.clone());
        }
        file.trim_append(render_with_hbs(
            super::template::Template::Client,
            &json!({
                "name": self.elem().name,
                "provider": self.elem().provider,
                "options": options,
            }),
        ));
        file.add_export(self.elem().name.clone());
        collector.finish_file();
    }
}
