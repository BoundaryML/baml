use std::{collections::BTreeMap, path::PathBuf};

use anyhow::{Context, Result};
use baml_db::baml_compiler_hir::{ItemId, file_item_tree, project_items};
use baml_project::ProjectDatabase;
use baml_workspace::discover_baml_files;
use clap::Args;

use crate::test_filter::TestFilter;

#[derive(Args, Clone, Debug)]
pub struct TestArgs {
    #[arg(long, help = "path/to/baml_src", default_value = ".")]
    pub from: PathBuf,

    /// Only list selected tests
    #[arg(long, default_value_t = false)]
    list: bool,

    #[arg(long, short = 'i')]
    /// Specific functions or tests to include. If none provided, runs all tests.
    ///
    /// Examples:
    ///
    /// -i "FunctionName::TestName" will match the specific test
    ///
    /// -i "FunctionName::" will run all tests in the function
    ///
    /// -i "::TestName" will run the test in any function
    ///
    /// -i "Get*::*Bar" will match with wildcards
    pub include: Vec<String>,

    #[arg(long, short = 'x')]
    /// Specific functions or tests to exclude. Takes precedence over --include.
    ///
    /// Uses the same syntax as --include.
    pub exclude: Vec<String>,
}

/// A discovered test: a (function_name, test_name) pair with its source location.
struct DiscoveredTest {
    function_name: String,
    test_name: String,
    file_path: PathBuf,
}

impl TestArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let from = std::fs::canonicalize(&self.from)
            .with_context(|| format!("Could not resolve baml_src path: {}", self.from.display()))?;

        // Set up the compiler database and load all .baml files.
        let mut db = ProjectDatabase::new();
        let baml_files = discover_baml_files(&from);
        if baml_files.is_empty() {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("No .baml files found in {}", from.display());
            }
            return Ok(crate::ExitCode::NoTestsRun);
        }

        for file_path in &baml_files {
            let content = std::fs::read_to_string(file_path)
                .with_context(|| format!("Failed to read {}", file_path.display()))?;
            db.add_or_update_file(file_path, &content);
        }
        let project = db.set_project_root(&from);

        // Discover all (function, test) pairs from the HIR.
        let discovered = discover_tests(&db, project);

        // Apply include/exclude filters.
        let filter = TestFilter::from(
            self.include.iter().map(|s| s.as_str()),
            self.exclude.iter().map(|s| s.as_str()),
        );

        let selected: BTreeMap<(String, String), PathBuf> = discovered
            .into_iter()
            .filter(|t| filter.includes(&t.function_name, &t.test_name))
            .map(|t| ((t.function_name, t.test_name), t.file_path))
            .collect();

        if selected.is_empty() {
            println!("No tests selected.");
            return Ok(crate::ExitCode::NoTestsRun);
        }

        if self.list {
            println!("Selected tests ({}):\n", selected.len());
            for ((func, test), path) in &selected {
                println!("  {func}::{test}  ({path})", path = path.display());
            }
            return Ok(crate::ExitCode::Success);
        }

        // TODO: Actual test execution (runtime invocation) goes here.
        println!(
            "Would run {} test(s), but runtime execution is not yet implemented.",
            selected.len()
        );
        for ((func, test), _) in &selected {
            println!("  {func}::{test}");
        }

        Ok(crate::ExitCode::Success)
    }
}

/// Walk the HIR to discover all (function_name, test_name) pairs.
///
/// Each test block references one or more functions via `function_refs`.
/// We expand each test into one entry per referenced function, matching
/// the old engine's `walk_function_test_pairs` behavior.
fn discover_tests(db: &ProjectDatabase, project: baml_workspace::Project) -> Vec<DiscoveredTest> {
    let items = project_items(db, project);
    let mut tests = Vec::new();

    for item in items.items(db) {
        if let ItemId::Test(test_loc) = item {
            let file = test_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let test = &item_tree[test_loc.id(db)];
            let file_path = file.path(db);

            for func_ref in &test.function_refs {
                tests.push(DiscoveredTest {
                    function_name: func_ref.to_string(),
                    test_name: test.name.to_string(),
                    file_path: file_path.clone(),
                });
            }
        }
    }

    tests
}
