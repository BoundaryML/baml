use colored::*;
use std::{
    io::{BufRead, BufReader},
    ops::Deref,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
    thread,
};

use baml_lib::{internal_baml_schema_ast::ast::WithName, Configuration, ValidatedSchema};
use log::info;

use crate::{
    command::{run_command, run_command_with_error},
    errors::CliError,
    test_command::test_state::RunState,
    TestAction, TestArgs,
};

mod ipc_comms;
mod run_test_with_forward;
mod run_test_with_watcher;
mod run_tests;
mod test_state;

enum Filter {
    Wildcard(glob::Pattern),
    // Function, Impl, Test
    Parts(glob::Pattern, glob::Pattern, glob::Pattern),
}

impl Filter {
    fn from_string(arg: &String) -> Result<Self, CliError> {
        // arg is of the form: "[function]:[impl]:[test]", if any of the fields are missing, they are
        // replaced with "*"

        if arg.contains(':') {
            let mut parts = arg.split(':');
            let function = parts.next().map(|s| s).unwrap_or(Default::default());
            let r#impl = parts.next().map(|s| s).unwrap_or(Default::default());
            let test = parts.next().map(|s| s).unwrap_or(Default::default());

            if parts.next().is_some() {
                panic!("Invalid filter: {}", arg);
            }

            // If any of the fields are missing or empty, replace them with "*"
            let function = if function.is_empty() { "*" } else { function };
            let r#impl = if r#impl.is_empty() { "*" } else { r#impl };
            let test = if test.is_empty() { "*" } else { test };

            let function = glob::Pattern::from_str(function)?;
            let r#impl = glob::Pattern::from_str(r#impl)?;
            let test = glob::Pattern::from_str(test)?;

            Ok(Filter::Parts(function, r#impl, test))
        } else {
            // If the string does not contain any glob characters, add * to the beginning and end
            let glob_chars = ['*', '?', '[', ']'];
            if !arg.chars().any(|c| glob_chars.contains(&c)) {
                return Ok(Filter::Wildcard(glob::Pattern::from_str(&format!(
                    "*{}*",
                    arg
                ))?));
            }
            Ok(Filter::Wildcard(glob::Pattern::from_str(arg)?))
        }
    }

    fn matches(&self, function: &str, r#impl: &str, test: &str) -> bool {
        match self {
            Filter::Wildcard(s) => s.matches(function) || s.matches(r#impl) || s.matches(test),
            Filter::Parts(f, i, t) => f.matches(function) && i.matches(r#impl) && t.matches(test),
        }
    }
}

fn matches_filters(
    function: &str,
    r#impl: &str,
    test: &str,
    includes: &Vec<Filter>,
    excludes: &Vec<Filter>,
) -> bool {
    let include = includes
        .iter()
        .any(|filter| filter.matches(function, r#impl, test));
    let exclude = excludes
        .iter()
        .any(|filter| filter.matches(function, r#impl, test));

    match (includes.is_empty(), excludes.is_empty()) {
        (true, true) => true,
        (true, false) => !exclude,
        (false, true) => include,
        (false, false) => include && !exclude,
    }
}

pub fn run(
    command: &TestArgs,
    baml_dir: &PathBuf,
    config: &Configuration,
    schema: ValidatedSchema,
) -> Result<(), CliError> {
    // process the args

    // Compute the list of tests to run
    let includes = command
        .include
        .iter()
        .filter_map(|s| Filter::from_string(s).ok())
        .collect::<Vec<_>>();
    let excludes = command
        .exclude
        .iter()
        .filter_map(|s| Filter::from_string(s).ok())
        .collect::<Vec<_>>();

    // Get selected tests
    let mut num_tests = 0;
    let mut num_selected_tests = 0;
    let selected_tests = schema
        .db
        .walk_test_cases()
        .flat_map(|test_case| {
            let funcwalker = test_case.walk_function();
            let function = funcwalker.name();
            let test = test_case.name();
            funcwalker
                .walk_variants()
                .filter_map(|variant| {
                    let r#impl = variant.name();
                    num_tests += 1;
                    if matches_filters(function, r#impl, test, &includes, &excludes) {
                        num_selected_tests += 1;
                        Some((function.to_string(), test.to_string(), r#impl.to_string()))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    // sort the tests
    let mut selected_tests = selected_tests;
    selected_tests.sort_by(|a, b| match a.0.cmp(&b.0) {
        std::cmp::Ordering::Equal => match a.1.cmp(&b.1) {
            std::cmp::Ordering::Equal => a.2.cmp(&b.2),
            x => x,
        },
        x => x,
    });

    let state = RunState::from_tests(schema.db, &selected_tests);

    // Print some information about the tests we are going to run
    let summary = format!(
        "================= {}/{} tests selected ({} deselected) =================",
        num_selected_tests,
        num_tests,
        num_tests - num_selected_tests
    )
    .green()
    .bold();

    println!("{summary}");
    match command.action {
        TestAction::Run => {
            // Selected config:
            let shell_setup = config.generators.iter().find_map(|(f, _)| {
                if f.language == "python" {
                    f.shell_setup.clone()
                } else {
                    None
                }
            });

            // Run the tests
            run_tests::run_tests(
                state,
                shell_setup,
                baml_dir,
                &selected_tests,
                command.playground_port,
            )
        }
        TestAction::List => {
            println!("{}", state.to_string());
            Ok(())
        }
    }?;

    println!("{}", summary);
    Ok(())
}
