//! `TestRegistry` — collected test metadata from a BAML program.
//!
//! `TestRegistry` holds a live `BexExternalValue::Handle` pointing to the
//! `testing.TestCollector` heap object (keeping it GC-rooted for later execution),
//! plus cached `Vec<TestInfo>` / `Vec<TestSetResult>` metadata extracted at
//! collection time.
//!
//! Callers can enumerate tests from the cached metadata without touching the
//! heap. The live handle is reserved for a future `run_tests` implementation.

use bex_external_types::BexExternalValue;

/// Metadata for a single test leaf.
pub struct TestInfo {
    pub name: String,
    pub body: BexExternalValue,
    pub runner: Option<BexExternalValue>,
}

/// Metadata for a testset node (may contain nested tests and testsets).
///
/// `tests` and `testsets` are fully populated during `collect_tests` by
/// invoking each testset's collector closure (without running test bodies).
pub struct TestSetInfo {
    pub name: String,
    pub tests: Vec<TestInfo>,
    pub testsets: Vec<TestSetResult>,
    /// Time in ms for this testset's collector closure only (not children).
    pub loading_time_ms: u64,
    /// Total wall-clock time in ms including all recursive child expansion.
    pub total_loading_time_ms: u64,
}

/// Result of expanding a testset stub — either fully expanded or kept lazy.
pub enum TestSetResult {
    Expanded(TestSetInfo),
    Lazy {
        name: String,
        collector_closure: Box<BexExternalValue>,
    },
}

/// A collected test tree.
///
/// Wraps a live `BexExternalValue::Handle` pointing to a `testing.TestCollector`
/// heap object, plus cached metadata extracted at collection time.
pub struct TestRegistry {
    /// The live heap reference — `BexExternalValue::Handle(handle)` (or
    /// `BexExternalValue::Null` for an empty registry with no tests).
    /// Kept alive so a follow-up `run_tests` can access test body lambdas.
    #[allow(dead_code)]
    handle: BexExternalValue,
    /// Flat list of top-level tests (from `registry.tests`).
    pub tests: Vec<TestInfo>,
    /// Top-level testsets with their full nested hierarchy (from `registry.testsets`).
    pub testsets: Vec<TestSetResult>,
}

impl TestRegistry {
    pub(crate) fn new(
        handle: BexExternalValue,
        tests: Vec<TestInfo>,
        testsets: Vec<TestSetResult>,
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
        fn collect_from_result(result: &TestSetResult, names: &mut Vec<String>) {
            match result {
                TestSetResult::Expanded(ts) => collect_from_testset(ts, names),
                TestSetResult::Lazy { .. } => {}
            }
        }
        fn collect_from_testset(ts: &TestSetInfo, names: &mut Vec<String>) {
            for t in &ts.tests {
                names.push(t.name.clone());
            }
            for nested in &ts.testsets {
                collect_from_result(nested, names);
            }
        }
        let mut names = Vec::new();
        for t in &self.tests {
            names.push(t.name.clone());
        }
        for ts in &self.testsets {
            collect_from_result(ts, &mut names);
        }
        names
    }
}
