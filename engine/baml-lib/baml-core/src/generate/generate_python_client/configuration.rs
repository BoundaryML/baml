use internal_baml_parser_database::walkers::RetryPolicyWalker;
use internal_baml_parser_database::RetryPolicyStrategy;
use internal_baml_schema_ast::ast::WithName;
use serde_json::json;

use crate::generate::generate_python_client::file::clean_file_name;

use super::{
    file::FileCollector,
    template::render_template,
    traits::{JsonHelper, WithWritePythonString},
};

impl WithWritePythonString for RetryPolicyWalker<'_> {
    fn file_name(&self) -> String {
        clean_file_name(self.ast_node().get_type())
    }

    fn write_py_file<'a>(&'a self, fc: &'a mut FileCollector) {
        fc.start_py_file("configs", "__init__");
        fc.last_file()
            .add_import(&format!(".{}", self.file_name()), self.name());
        fc.complete_file();

        fc.start_py_file("configs", self.file_name());
        let json = self.json(fc.last_file());
        render_template(
            super::template::HSTemplate::RetryPolicy,
            fc.last_file(),
            json,
        );
        fc.complete_file();
    }
}

impl JsonHelper for RetryPolicyWalker<'_> {
    fn json(&self, f: &mut super::file::File) -> serde_json::Value {
        let strategy = match &self.config().strategy {
            RetryPolicyStrategy::ConstantDelay(strategy) => {
                f.add_import(
                    "baml_core.configs.retry_policy",
                    "create_retry_policy_constant_delay",
                );
                json!({
                    "type": "constant_delay",
                    "params": {
                      "delay_ms": strategy.delay_ms,
                    }
                })
            }
            RetryPolicyStrategy::ExponentialBackoff(strategy) => {
                f.add_import(
                    "baml_core.configs.retry_policy",
                    "create_retry_policy_exponential_backoff",
                );
                json!({
                    "type": "exponential_backoff",
                    "params": {
                        "delay_ms": strategy.delay_ms,
                        "max_delay_ms": strategy.max_delay_ms,
                        "multiplier": strategy.multiplier,
                    }
                })
            }
        };

        json!({
            "name": self.name(),
            "max_retries": self.config().max_retries,
            "strategy": strategy,
        })
    }
}
