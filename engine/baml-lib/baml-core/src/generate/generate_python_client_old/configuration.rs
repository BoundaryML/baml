use internal_baml_parser_database::walkers::ConfigurationWalker;
use internal_baml_parser_database::RetryPolicyStrategy;
use internal_baml_schema_ast::ast::{Configuration, FunctionArgs, WithName};
use serde_json::json;

use crate::generate::generate_python_client_old::file::clean_file_name;

use super::{
    file::FileCollector,
    template::{render_template, HSTemplate},
    traits::{JsonHelper, WithWritePythonString},
    value::to_py_value,
    WithToCode,
};

impl WithWritePythonString for ConfigurationWalker<'_> {
    fn file_name(&self) -> String {
        clean_file_name(match self.id.1 {
            "retry_policy" => "retry_policy",
            "test_case" => "test_baml_client",
            "printer" => "printer",
            _ => unreachable!("Invalid configuration type"),
        })
    }

    fn write_py_file<'a>(&'a self, fc: &'a mut FileCollector) {
        if self.id.1 == "printer" {
            // Printers aren't generated.
            return;
        }

        match self.ast_node() {
            Configuration::RetryPolicy(_) => {
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
            Configuration::Printer(_) => {}
            Configuration::TestCase(tc) => {
                let func = match self.walk_function() {
                    Some(func) => func,
                    None => {
                        eprintln!(
                            "Function {} not found for test case {}",
                            self.test_case().function.0,
                            tc.name()
                        );
                        return;
                    }
                };

                fc.start_export_file("tests", "__init__");
                fc.complete_file();

                fc.start_export_file("tests", format!("test_{}", func.name()));

                func.walk_input_args().for_each(|arg| {
                    arg.required_classes().for_each(|class| {
                        fc.last_file().add_import("..baml_types", class.name());
                    });
                    arg.required_enums().for_each(|enum_| {
                        fc.last_file().add_import("..baml_types", enum_.name());
                    });
                });
                func.walk_output_args().for_each(|arg| {
                    arg.required_classes().for_each(|class| {
                        fc.last_file().add_import("..baml_types", class.name());
                    });
                    arg.required_enums().for_each(|enum_| {
                        fc.last_file().add_import("..baml_types", enum_.name());
                    });
                });
                fc.last_file()
                    .add_import("..baml_types", &format!("I{}Stream", func.name()));
                fc.last_file()
                    .add_import("..baml_types", &format!("I{}", func.name()));
                fc.last_file()
                    .add_import("pytest_baml.ipc_channel", "BaseIPCChannel");
                let test_case_content =
                    to_py_value(&Into::<serde_json::Value>::into(&self.test_case().content));
                match func.ast_function().input() {
                    FunctionArgs::Unnamed(arg) => {
                        let data = json!({
                            "function_name": func.name(),
                            "test_case_name": tc.name(),
                            "test_case_input": test_case_content,
                            "test_case_type": arg.to_py_string(fc.last_file()),
                            "is_streaming_supported": self.is_streaming_supported(),
                        });
                        render_template(HSTemplate::SingleArgTestSnippet, fc.last_file(), data);
                    }
                    FunctionArgs::Named(args) => {
                        let data = json!({
                            "function_name": func.name(),
                            "test_case_name": tc.name(),
                            "test_case_input": test_case_content,
                            "test_case_types": args.args.iter().map(|(k, v)| json!({
                                "name": k.name(),
                                "type": v.to_py_string(fc.last_file()),
                             })).collect::<Vec<_>>(),
                             "is_streaming_supported": self.is_streaming_supported(),

                        });
                        render_template(HSTemplate::MultiArgTestSnippet, fc.last_file(), data);
                    }
                }
                fc.complete_file();
            }
        }
    }
}

impl JsonHelper for ConfigurationWalker<'_> {
    fn json(&self, f: &mut super::file::File) -> serde_json::Value {
        match self.id.1 {
            "retry_policy" => {
                let strategy = match &self.retry_policy().strategy {
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
                    "max_retries": self.retry_policy().max_retries,
                    "strategy": strategy,
                })
            }
            "test_case" => {
                json!({
                    "name": self.name(),
                    "function": self.test_case().function.0,
                    "test_case": Into::<serde_json::Value>::into(&self.test_case().content),
                })
            }
            "printer" => {
                json!({
                    "name": self.name(),
                    "printer": self.printer().template(),
                })
            }
            _ => unreachable!("Invalid configuration type"),
        }
    }
}
