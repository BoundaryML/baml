use crate::errors::CliError;

use serde_json::json;

use std::io::Write;
use std::path::PathBuf;

use tempfile::Builder;
use uuid::Uuid;

use super::{
    run_test_with_forward::run_test_with_forward, run_test_with_watcher::run_test_with_watcher,
    test_state::RunState,
};

use tempfile::NamedTempFile;

#[derive(Debug, Clone)]
pub(crate) enum TestRunner {
    Pytest,
    Jest,
}

impl TestRunner {
    pub fn run<T: AsRef<str>>(
        &self,
        state: RunState,
        output_path: &PathBuf,
        test_command: T,
        // function, test, impl
        selected_tests: &Vec<(String, String, String)>,
        playground_port: Option<u16>,
    ) -> Result<(), CliError> {
        let mut test_command = shellwords::split(test_command.as_ref())
            .map_err(|e| CliError::StringError(format!("Failed to parse test command: {}", e)))?;

        match self {
            TestRunner::Jest => {
                let baml_test_config_file: Result<NamedTempFile, std::io::Error> = Builder::new()
                    .prefix("baml_test_config")
                    .suffix(".json")
                    .rand_bytes(5)
                    .tempfile();

                // Use two different files since jest complains if it detects a key that is not part
                // of its configuration.
                let jest_config_file: Result<NamedTempFile, std::io::Error> = Builder::new()
                    .prefix("jest_config")
                    .suffix(".json")
                    .rand_bytes(5)
                    .tempfile();
                let baml_error_message = "Failed to write baml test config to temporary file";
                let jest_error_message = "Failed to write jest config to temporary file";
                match (baml_test_config_file, jest_config_file) {
                    (Ok(mut baml_file), Ok(mut jest_file)) => {
                        let baml_file_write_result = baml_file.write_all(
                            json!({
                                "expected_tests": expected_tests_array(selected_tests),
                            })
                            .to_string()
                            .as_bytes(),
                        );

                        let jest_file_write_result = jest_file.write_all(
                            json!({
                                "testNamePattern": jest_test_pattern(selected_tests),
                                "detectOpenHandles": true,
                                "runner": "@boundaryml/baml-core/baml_test_runner",
                                "preset": "ts-jest"
                            })
                            .to_string()
                            .as_bytes(),
                        );

                        match (baml_file_write_result, jest_file_write_result) {
                            (Err(e), _) => {
                                eprintln!("{}: {}", baml_error_message, e);
                                return Err(CliError::StringError(baml_error_message.to_string()));
                            }
                            (_, Err(e)) => {
                                eprintln!("{}: {}", jest_error_message, e);
                                return Err(CliError::StringError(jest_error_message.to_string()));
                            }
                            _ => {}
                        }
                        let baml_temp_path = baml_file.path().to_str().unwrap().to_string();
                        let jest_temp_path = jest_file.path().to_str().unwrap().to_string();

                        test_command.push(format!("--baml-test-config-file={}", baml_temp_path));
                        test_command
                            .push(format!("--rootDir=\"{}\"", output_path.to_string_lossy()));
                        test_command.push(format!("--config=\"{}\"", jest_temp_path));

                        let res = match playground_port {
                            Some(port) => run_test_with_forward(
                                self.clone(),
                                output_path.clone(),
                                state,
                                test_command,
                                port,
                            ),
                            None => run_test_with_watcher(
                                self.clone(),
                                output_path.clone(),
                                state,
                                test_command,
                            ),
                        };

                        let _ = baml_file.close();
                        let _ = jest_file.close();
                        res
                    }
                    (Err(e), _) => {
                        eprintln!("{}: {}", baml_error_message, e);
                        Err(CliError::StringError(baml_error_message.to_string()))
                    }
                    (_, Err(e)) => {
                        eprintln!("{}: {}", jest_error_message, e);
                        Err(CliError::StringError(jest_error_message.to_string()))
                    }
                    _ => {
                        let general_error_message =
                            "Failed to create temporary files for jest and baml test config";
                        eprintln!("{}", general_error_message);
                        Err(CliError::StringError(general_error_message.to_string()))
                    }
                }
            }
            TestRunner::Pytest => {
                test_command.push(output_path.to_string_lossy().to_string());
                selected_tests.iter().for_each(|(function, test, r#impl)| {
                    test_command.push("--pytest-baml-include".into());
                    test_command.push(format!("{}:{}:{}", function, r#impl, test));
                });

                match playground_port {
                    Some(port) => run_test_with_forward(
                        self.clone(),
                        output_path.clone(),
                        state,
                        test_command,
                        port,
                    ),
                    None => run_test_with_watcher(
                        self.clone(),
                        output_path.clone(),
                        state,
                        test_command,
                    ),
                }
            }
        }
    }

    pub fn add_ipc_to_command(&self, command: &mut Vec<String>, port: u16) {
        match self {
            TestRunner::Jest => {
                command.push(format!("--baml-ipc={}", port));
            }
            TestRunner::Pytest => {
                command.push("--pytest-baml-ipc".into());
                command.push(format!("{}", port));
            }
        }
    }

    pub fn env_vars(&self) -> Vec<(String, String)> {
        let uuid = Uuid::new_v4().to_string();
        let env_vars = vec![
            ("BOUNDARY_PRINT_EVENTS".to_string(), "1".to_string()),
            ("BOUNDARY_PROCESS_ID".to_string(), uuid),
        ];
        match self {
            TestRunner::Jest => env_vars,
            TestRunner::Pytest => env_vars,
        }
    }
}

// input is  func, test, impl
fn expected_tests_array(tests: &Vec<(String, String, String)>) -> Vec<Vec<&String>> {
    let mut test_array = Vec::new();
    for (function, test, r#impl) in tests {
        test_array.push(vec![test, r#impl, function]);
    }
    test_array
}

fn jest_test_pattern(tests: &Vec<(String, String, String)>) -> String {
    let test_patterns: Vec<String> = tests
        .iter()
        .map(|(function, test, r#impl)| {
            format!("test_case:{} function:{} impl:{}", test, function, r#impl)
        })
        .collect();
    let test_pattern = format!("({})", test_patterns.join("|"));
    test_pattern
}
