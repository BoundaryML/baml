use indexmap::IndexMap;
use serde_json::json;

use crate::generate::{
    dir_writer::WithFileContentPy as WithFileContent,
    ir::{Client, Expression, Identifier, Walker},
};

use super::{
    python_language_features::{PythonFileCollector, PythonLanguageFeatures, ToPython},
    template::render_with_hbs,
};

impl WithFileContent<PythonLanguageFeatures> for Walker<'_, &Client> {
    fn file_dir(&self) -> &'static str {
        "."
    }

    fn file_name(&self) -> String {
        "client".into()
    }

    fn write(&self, collector: &mut PythonFileCollector) {
        let file = collector.start_file(self.file_dir(), self.file_name(), false);
        // file.add_import(
        //     "@boundaryml/baml-core/client_manager",
        //     "clientManager",
        //     None,
        //     false,
        // );
        println!("client: {:#?}", self.elem());

        let opts = self
            .elem()
            .options
            .iter()
            .map(|(k, v)| {
                json!({
                  "key": k.clone(),
                  "value": v.to_py(),
                })
            })
            .collect::<Vec<_>>();

        let redactions = self
            .elem()
            .options
            .iter()
            .filter_map(|(k, v)| match v {
                Expression::Identifier(s) => match s {
                    Identifier::ENV(_) => Some(format!("\"{}\"", k)),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(", ");

        file.append(render_with_hbs(
            super::template::Template::Client,
            &json!({
                "name": self.elem().name,
                "kwargs": {
                  "provider": format!("\"{}\"", self.elem().provider),
                  "retry_policy": self.elem().retry_policy_id,
                  "redactions": format!("[{}]", redactions),
                },
                "provider": self.elem().provider,
                // TODO: just pass in the expression and inline it directly -- no need to separate into key and value.
                // to do this update the ToPython trait for Expression and make sure it does the right thing for maps.
                "options": opts,
                // TODO: add the doc string
                //"doc_string": None,
            }),
        ));
        file.add_export(self.elem().name.clone());
        collector.finish_file();
    }
}
