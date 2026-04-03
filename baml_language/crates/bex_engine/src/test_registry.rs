//! `TestRegistry` — collected test metadata from a BAML program.
//!
//! `TestRegistry` holds a live `BexExternalValue::Handle` pointing to the
//! `testing.Registry` heap object (keeping it GC-rooted for later execution),
//! plus cached `Vec<TestInfo>` / `Vec<TestSetInfo>` metadata extracted at
//! collection time.
//!
//! Callers can enumerate tests from the cached metadata without touching the
//! heap. The live handle is reserved for a future `run_tests` implementation.

use bex_external_types::BexExternalValue;

/// Metadata for a single test leaf.
#[derive(Debug, Clone)]
pub struct TestInfo {
    pub name: String,
}

/// Metadata for a testset node (may contain nested tests and testsets).
///
/// `tests` and `testsets` are populated lazily in the `run_tests`
/// follow-up — `collect_tests` only extracts top-level names (executing
/// nested testset collectors requires running the collector closure, which
/// is test *execution* logic, not collection logic).
#[derive(Debug, Clone)]
pub struct TestSetInfo {
    pub name: String,
    pub tests: Vec<TestInfo>,
    pub testsets: Vec<TestSetInfo>,
}

/// A collected test tree.
///
/// Wraps a live `BexExternalValue::Handle` pointing to a `testing.Registry`
/// heap object, plus cached metadata extracted at collection time.
#[derive(Debug)]
pub struct TestRegistry {
    /// The live heap reference — `BexExternalValue::Handle(handle)` (or
    /// `BexExternalValue::Null` for an empty registry with no tests).
    /// Kept alive so a follow-up `run_tests` can access test body lambdas.
    #[allow(dead_code)]
    handle: BexExternalValue,
    /// Flat list of top-level tests (from `registry.tests`).
    pub tests: Vec<TestInfo>,
    /// Top-level testsets with their names (from `registry.testsets`).
    /// Nested hierarchy is not populated at collection time.
    pub testsets: Vec<TestSetInfo>,
}

impl TestRegistry {
    pub(crate) fn new(
        handle: BexExternalValue,
        tests: Vec<TestInfo>,
        testsets: Vec<TestSetInfo>,
    ) -> Self {
        Self {
            handle,
            tests,
            testsets,
        }
    }

    /// Empty registry — returned when a package has no test blocks.
    pub(crate) fn empty() -> Self {
        Self {
            handle: BexExternalValue::Null,
            tests: vec![],
            testsets: vec![],
        }
    }

    /// All leaf test names (flat), including those nested inside testsets.
    ///
    /// Tests registered directly on the root registry come first, followed
    /// by tests from each top-level testset (depth-first).
    pub fn all_test_names(&self) -> Vec<String> {
        fn collect_from_testset(ts: &TestSetInfo, names: &mut Vec<String>) {
            for t in &ts.tests {
                names.push(t.name.clone());
            }
            for nested in &ts.testsets {
                collect_from_testset(nested, names);
            }
        }
        let mut names = Vec::new();
        for t in &self.tests {
            names.push(t.name.clone());
        }
        for ts in &self.testsets {
            collect_from_testset(ts, &mut names);
        }
        names
    }
}
