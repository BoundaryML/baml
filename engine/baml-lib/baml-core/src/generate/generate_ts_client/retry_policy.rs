use crate::generate::{
    dir_writer::WithFileContentTs,
    ir::{RetryPolicy, Walker},
};

use super::ts_language_features::{TSFileCollector, TSLanguageFeatures, ToTypeScript};
use internal_baml_parser_database::RetryPolicyStrategy;

impl ToTypeScript for RetryPolicy {
    fn to_ts(&self) -> String {
        let name = &self.elem.name.0;

        match self.elem.strategy {
            RetryPolicyStrategy::ConstantDelay(constant_delay) => format!(
                "const {name} = new BamlInternalConstantDelayRetryPolicy(\"{name}\", {}, {});",
                self.elem.max_retries,
                serde_json::to_string_pretty(&constant_delay)
                    .unwrap_or("/* Error serializing constant_delay */".into())
            ),
            RetryPolicyStrategy::ExponentialBackoff(exponential_backoff) => format!(
                "const {name} = new BamlInternalExponentialBackoffRetryPolicy(\"{name}\", {}, {});",
                self.elem.max_retries,
                serde_json::to_string_pretty(&exponential_backoff)
                    .unwrap_or("/* Error serializing constant_delay */".into())
            ),
        }
    }
}

impl WithFileContentTs<TSLanguageFeatures> for Walker<'_, &RetryPolicy> {
    fn file_dir(&self) -> &'static str {
        "."
    }

    fn file_name(&self) -> String {
        "retry_policy".into()
    }

    fn write(&self, collector: &mut TSFileCollector) {
        let file = collector.start_file(self.file_dir(), self.file_name(), false);
        // NB: it's silly that we're renaming these to "BamlInternal" but add_import doesn't
        // support importing an entire module, only importing symbols from a module, so this
        // is an easy workaround
        //
        // The purpose of this is to free up symbols like "ConstantDelayRetryPolicy" for end-user
        // usage
        file.add_import(
            "@boundaryml/baml-core/client_manager",
            "ConstantDelayRetryPolicy",
            Some("BamlInternalConstantDelayRetryPolicy"),
            false,
        );
        file.add_import(
            "@boundaryml/baml-core/client_manager",
            "ExponentialBackoffRetryPolicy",
            Some("BamlInternalExponentialBackoffRetryPolicy"),
            false,
        );
        file.trim_append(self.item.to_ts());
        file.add_export(self.elem().name.0.clone());
        collector.finish_file();
    }
}
