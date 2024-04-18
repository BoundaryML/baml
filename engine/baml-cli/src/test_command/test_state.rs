use baml_lib::internal_baml_parser_database::ParserDatabase;
use colored::*;
use log::debug;
use std::{collections::HashMap, ops::Deref, str::FromStr};

use super::ipc_comms::{LogSchema, MessageData, Template, TestCaseStatus, ValueType};

#[derive(Debug)]
enum TestState {
    Queued,
    Running,
    Cancelled,
    // bool is true if the test passed
    Finished(FinishedState),
}

#[derive(Debug)]
struct FinishedState {
    passed: bool,
    log: Option<LogSchema>,
}

enum ExecutorStage {
    Ready,
    Parsed,
    #[allow(dead_code)]
    Running,
    #[allow(dead_code)]
    Finished,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct TestSpec {
    function: String,
    test: String,
    r#impl: String,
}

pub(crate) struct RunState {
    schema: ParserDatabase,
    messages: Vec<MessageData>,
    dashboard_url: Option<String>,
    // If pytest got past the parsing stage
    executor_stage: ExecutorStage,
    // Test state for each test
    // Function, Test, Impl
    tests: HashMap<TestSpec, TestState>,

    // Collection summary (used for simplified printing)
    collected: Vec<(String, Vec<(String, Vec<String>)>, Vec<String>)>,
}

impl RunState {
    pub(crate) fn from_tests(db: ParserDatabase, tests: &Vec<(String, String, String)>) -> Self {
        let mut collected = Vec::new();
        tests.iter().for_each(|(function, test, r#impl)| {
            // IF the last collected function is different from the current function, add it in.
            if collected.last().map(|(f, _, _)| f) != Some(function) {
                collected.push((function.clone(), Vec::new(), Vec::new()));
            }
            // If the last collected test is different from the current test, add it in.
            if collected.last().unwrap().1.last().map(|(t, _)| t) != Some(test) {
                collected
                    .last_mut()
                    .unwrap()
                    .1
                    .push((test.clone(), Vec::new()));
            }
            // Add the impl to the last collected test
            collected
                .last_mut()
                .unwrap()
                .1
                .last_mut()
                .unwrap()
                .1
                .push(r#impl.clone());
        });

        // For every test, if all the impls are the same, add them to the function impls list
        collected.iter_mut().for_each(|(_, tests, func_impls)| {
            // impls for the first test
            let mut impls = tests
                .first()
                .map(|(_, impls)| impls.clone())
                .unwrap_or_default();
            // For every test, if all the impls are the same, add them to the function impls list
            if tests.iter().skip(1).all(|(_, test_impls)| {
                // Ensure that the impls are the same (arrays are sorted)
                impls == test_impls.deref()
            }) {
                func_impls.append(&mut impls);
            }
        });

        Self {
            schema: db,
            messages: Vec::new(),
            dashboard_url: None,
            executor_stage: ExecutorStage::Ready,
            tests: tests
                .iter()
                .map(|(function, test, r#impl)| {
                    (
                        TestSpec {
                            function: function.clone(),
                            test: test.clone(),
                            r#impl: r#impl.clone(),
                        },
                        TestState::Queued,
                    )
                })
                .collect(),
            collected,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        // Every test should have a state that is not Queued

        let errors: Vec<_> = self
            .tests
            .iter()
            .filter(|(_, state)| {
                matches!(
                    state,
                    TestState::Queued | TestState::Running | TestState::Cancelled
                )
            })
            .collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "Unexpected error! Please report a bug on our github.\nThe following tests are still pending: {}",
                errors.iter().map(|(spec, _)| format!("\t{}:{}:{}", spec.function, spec.r#impl, spec.test)).collect::<Vec<_>>().join("\n")
            ))
        }
    }

    pub(crate) fn sync(&mut self) -> Option<String> {
        let mut additional = HashMap::new();
        let messages = std::mem::take(&mut self.messages);
        messages
            .into_iter()
            .for_each(|message| match self.update_test_state(message) {
                Some((spec, log)) => {
                    additional.insert(spec, log);
                }
                None => {}
            });
        if !additional.is_empty() {
            let mut output = String::new();
            for (spec, log) in additional {
                let formatted_header = format!(
                    "######## {} {}{}{} {} #############",
                    spec.function, "(impl: ", spec.r#impl, ")", spec.test,
                )
                .cyan()
                .bold();

                output += &format!("{}\n{}\n", formatted_header, log);
            }
            Some(output)
        } else {
            None
        }
    }

    // First string is the regular state, second string is any additional messages to print only once.
    pub(crate) fn to_string(&self) -> String {
        let output_iter = self.collected.iter().map(|(function, tests, impls)| {
            if impls.is_empty() {
                let mut output = format!(
                    "{} {}{} {}\n",
                    function.bold().cyan(),
                    "(".dimmed(),
                    tests.iter().map(|(_, v)| v.len()).sum::<usize>(),
                    "tests)".dimmed()
                );
                for (test, impls) in tests {
                    output += &format!("  {} {}", test, "(impls:".dimmed());
                    for impl_ in impls {
                        let state = self.tests.get(&TestSpec {
                            function: function.clone(),
                            test: test.clone(),
                            r#impl: impl_.clone(),
                        });

                        match state {
                            Some(TestState::Queued) => {
                                output += &format!(" {}{}", "○".dimmed(), impl_.dimmed());
                            }
                            Some(TestState::Running) => {
                                output += &format!(" {}{}", "●".dimmed(), impl_.dimmed());
                            }
                            Some(TestState::Cancelled) => {
                                output += &format!(" {}{}", "✕".dimmed(), impl_.dimmed());
                            }
                            Some(TestState::Finished(state)) => {
                                if state.passed {
                                    output += &format!(" {}{}", "✔".green(), impl_.dimmed());
                                } else {
                                    output += &format!(" {}{}", "✖".red(), impl_.dimmed());
                                }
                            }
                            None => {
                                output += &format!(" {}", impl_.dimmed());
                            }
                        }
                    }
                    output += &format!("{}", ")\n".dimmed());
                }
                output
            } else {
                let mut output = format!(
                    "{} {} {}{} {}{} {}\n",
                    function.bold().cyan(),
                    "(impls:".dimmed(),
                    impls.join(", ").dimmed(),
                    ")".dimmed(),
                    "(".dimmed(),
                    tests.len() * impls.len(),
                    "tests)".dimmed()
                );
                for (test, impls) in tests {
                    output += &format!("  {}", test);
                    for impl_ in impls {
                        let state = self.tests.get(&TestSpec {
                            function: function.clone(),
                            test: test.clone(),
                            r#impl: impl_.clone(),
                        });
                        match state {
                            Some(TestState::Queued) => {
                                output += &format!(" {}", "○".dimmed());
                            }
                            Some(TestState::Running) => {
                                output += &format!(" {}", "●".dimmed());
                            }
                            Some(TestState::Cancelled) => {
                                output += &format!(" {}", "✕".dimmed());
                            }
                            Some(TestState::Finished(state)) => {
                                if state.passed {
                                    output += &format!(" {}", "✔".green());
                                } else {
                                    output += &format!(" {}", "✖".red());
                                }
                            }
                            None => {}
                        }
                    }
                    output += "\n";
                }
                output
            }
        });

        let output = output_iter.collect::<String>();

        if let Some(d) = &self.dashboard_url {
            format!(
                "\n\n{}\n{}\n{}\n{}\n\n",
                "####### Dashboard #######".dimmed(),
                d.white(),
                "#########################".dimmed(),
                output
            )
        } else {
            output
        }
    }

    pub(crate) fn add_message(&mut self, message: MessageData) {
        self.messages.push(message);
    }

    fn update_test_state(&mut self, message: MessageData) -> Option<(TestSpec, String)> {
        match message {
            MessageData::TestRunMeta(meta) => {
                self.dashboard_url = Some(meta.dashboard_url);
                self.executor_stage = ExecutorStage::Parsed;
                None
            }
            MessageData::UpdateTestCase(update) => {
                // update.test_case_arg_name is of form "test_<test>[<function>-<impl>]"
                let mut parts = update.test_case_arg_name.split('[');
                let test = parts.next().unwrap();
                // Strip leading "test_"
                let test = &test[5..];

                let mut parts = parts.next().unwrap().split(']');
                let mut parts = parts.next().unwrap().split('-');
                let function = parts.next().unwrap();
                let r#impl = parts.next().unwrap();

                let key = TestSpec {
                    function: function.into(),
                    test: test.into(),
                    r#impl: r#impl.into(),
                };

                let new_state = match update.status {
                    TestCaseStatus::Queued => TestState::Queued,
                    TestCaseStatus::Running => TestState::Running,
                    TestCaseStatus::Cancelled => TestState::Cancelled,
                    TestCaseStatus::ExpectedFailure => TestState::Finished(FinishedState {
                        passed: true,
                        log: None,
                    }),
                    TestCaseStatus::Passed => TestState::Finished(FinishedState {
                        passed: true,
                        log: None,
                    }),
                    TestCaseStatus::Failed => TestState::Finished(FinishedState {
                        passed: false,
                        log: None,
                    }),
                };

                let state = if let Some(x) = self.tests.get_mut(&key) {
                    debug!("Updating test state for {:?}", x);
                    x
                } else {
                    debug!("Unable to find test state for {:?}", key);
                    return None;
                };
                *state = new_state;

                update
                    .error_data
                    .map(|error| (key, format!("{}", error.to_string().red())))
            }
            MessageData::Log(log) => {
                // Log messages always come after the test case update
                {
                    let test_name = log.context.tags.get("test_case_arg_name");
                    if let Some(test_name) = test_name {
                        let mut parts = test_name.split('[');
                        let test = parts.next().unwrap();
                        // Strip leading "test_"
                        let test = &test[5..];

                        let mut parts = parts.next().unwrap().split(']');
                        let mut parts = parts.next().unwrap().split('-');
                        let function = parts.next().unwrap();
                        let r#impl = parts.next().unwrap();

                        Some(TestSpec {
                            function: function.into(),
                            test: test.into(),
                            r#impl: r#impl.into(),
                        })
                    } else {
                        None
                    }
                }
                .and_then(|spec| {
                    let state = self.tests.get_mut(&spec);
                    if let Some(state) = state {
                        if let TestState::Finished(state) = state {
                            let (llm_prompt, llm_raw_output) =
                                match log.metadata.as_ref().map(|meta| {
                                    // TODO: Swap out template vars
                                    let input = match &meta.input.prompt.template {
                                        Template::Single(o) => o.clone(),
                                        Template::Multiple(chats) => chats
                                            .iter()
                                            .map(|c| {
                                                format!(
                                                    "{}:\n{}",
                                                    c.role.as_str().yellow().bold(),
                                                    c.content.white()
                                                )
                                            })
                                            .collect::<Vec<_>>()
                                            .join("\n"),
                                    };

                                    let mut colored_input = input.clone();
                                    meta.input.prompt.template_args.iter().for_each(|(k, v)| {
                                        let replacement = format!("{}", v.blue()); // Colorize the replacement text in magenta
                                        colored_input = colored_input.replace(k, &replacement);
                                    });

                                    let raw_output =
                                        meta.output.as_ref().map(|output| output.raw_text.clone());

                                    (colored_input, raw_output)
                                }) {
                                    Some((llm_prompt, llm_raw_output)) => {
                                        (Some(llm_prompt), llm_raw_output)
                                    }
                                    None => (None, None),
                                };

                            let err = log.error.as_ref().map(|error| match &error.traceback {
                                Some(traceback) => {
                                    format!("{}\n{}", error.message, traceback)
                                }
                                None => error.message.clone(),
                            });

                            let parsed_output = match log.io.output.as_ref().map(|output| {
                                let output = match &output.value {
                                    ValueType::String(s) => serde_json::Value::from_str(s),
                                    ValueType::List(l) => l
                                        .iter()
                                        .map(|v| serde_json::Value::from_str(v))
                                        .collect::<Result<Vec<_>, _>>()
                                        .map(serde_json::Value::Array),
                                }
                                .map(|v| {
                                    serde_json::to_string_pretty(&v).unwrap_or_else(|_| {
                                        format!("Failed to serialize output: {:?}", v)
                                    })
                                })
                                .ok();
                                let r#type =
                                    self.schema.find_function_by_name(&spec.function).map(|f| {
                                        f.walk_output_args()
                                            .map(|w| w.ast_arg().1.field_type.to_string())
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    });
                                (output, r#type)
                            }) {
                                Some((Some(output), Some(r#type))) => Some((output, r#type)),
                                _ => None,
                            };

                            state.log = Some(log);

                            let res = match (llm_prompt, llm_raw_output, err, parsed_output) {
                                (Some(llm_prompt), Some(llm_raw_output), Some(err), _) => vec![
                                    format!("\n{}", "---- Prompt ---------".dimmed()),
                                    format!("{}", llm_prompt),
                                    format!("\n{}", "---- Raw Response ---".dimmed()),
                                    format!("{}", llm_raw_output.white()),
                                    format!("{}", "----- Error -----".dimmed()),
                                    format!("{}", err.red()),
                                ],
                                (Some(llm_prompt), None, Some(err), _) => vec![
                                    format!("\n{}", "---- Prompt ---------".dimmed()),
                                    format!("{}", llm_prompt),
                                    format!("{}", "----- Error -----".dimmed()),
                                    format!("{}", err.red()),
                                ],
                                (
                                    Some(llm_prompt),
                                    Some(llm_raw_output),
                                    None,
                                    Some((output, output_type)),
                                ) => {
                                    vec![
                                        format!("\n{}", "------- Prompt ------".yellow()),
                                        format!("{}", llm_prompt),
                                        format!("\n{}", "---- Raw Response ---".dimmed()),
                                        format!("{}", llm_raw_output.dimmed()),
                                        format!(
                                            "\n{}{}{}",
                                            "----- Parsed Response (".green(),
                                            output_type.green(),
                                            ") -----".green()
                                        ),
                                        format!("{}", output.green()),
                                    ]
                                }
                                _ => vec![],
                            }
                            .join("\n");

                            if !res.is_empty() {
                                return Some((spec, res));
                            }
                        } else {
                            println!("Test state is not finished: {:?} {:?}", spec, state);
                        }
                    }
                    None
                })
            }
            MessageData::PartialData(_v) => {
                // Test CLI doesn't use partial data
                None
            }
        }
    }
}

pub(crate) async fn on_finish_test(
    output: std::process::Output,
    state: &RunState,
    stdout_file_path: std::path::PathBuf,
    stderr_file_path: std::path::PathBuf,
) -> Result<(), std::io::Error> {
    let validated = state.validate();
    let status_code = output.status.code();
    let should_pipe_logs = match status_code {
        Some(0) => false,
        Some(_) => validated.is_err(),
        None => true,
    };

    if should_pipe_logs {
        let stdout_content = tokio::fs::read_to_string(&stdout_file_path).await?;
        let stderr_content = tokio::fs::read_to_string(&stderr_file_path).await?;
        println!(
            "\n####### STDOUT Logs for this test ########\n{}",
            stdout_content.bright_red().bold()
        );
        println!(
            "\n####### STDERR Logs for this test ########\n{}",
            stderr_content.bright_red().bold()
        );
    }

    match validated {
        Ok(_) => {
            if let Some(code) = status_code {
                if code == 0 {
                    Ok(())
                } else {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Testing failed with exit code {}", code),
                    ))
                }
            } else {
                Ok(())
            }
        }
        Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
    }
}
