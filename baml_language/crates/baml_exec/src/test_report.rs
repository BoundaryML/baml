// The `baml test` report contract, shared with the test binaries `baml pack
// --tests` produces: the `testing.TestRegistry.run_filtered` call, the
// `FlatTestReport` it returns, and the two renderings of that report — the
// CLI's, with per-leaf durations, and the byte-stable one a packed binary
// prints so two runs can be compared verbatim.
//
// `testing.TestRegistry.run_filtered(registry, profile filters, CLI filters,
// max_concurrency, timeout_ms)` and `testing.TestRegistry.list_filtered(...)`
// are defined in baml_std/testing/registry.baml. `self` is passed as the
// first positional argument (the live registry handle).

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{fmt::Write as _, sync::Arc, time::Duration};

use anyhow::{Result, anyhow};
use bex_engine::WireTy;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContext};

/// The selector layers a test run takes. Profile includes establish the
/// candidates and command-line includes narrow them; excludes accumulate
/// and always win. Matching happens in the stdlib (`testing.leaf_selected`).
#[derive(Clone, Debug, Default)]
pub struct TestSelection {
    pub profile_include: Vec<String>,
    pub profile_exclude: Vec<String>,
    pub cli_include: Vec<String>,
    pub cli_exclude: Vec<String>,
}

impl TestSelection {
    pub fn has_filters(&self) -> bool {
        !self.profile_include.is_empty()
            || !self.profile_exclude.is_empty()
            || !self.cli_include.is_empty()
            || !self.cli_exclude.is_empty()
    }

    /// The four selector lists in the order `run_filtered` / `list_filtered`
    /// take them, after the registry handle.
    fn args(&self) -> [BexExternalValue; 4] {
        [
            string_array(&self.profile_include),
            string_array(&self.profile_exclude),
            string_array(&self.cli_include),
            string_array(&self.cli_exclude),
        ]
    }
}

/// `baml test`'s default per-test deadline.
pub const DEFAULT_TEST_TIMEOUT: Duration = Duration::from_mins(5);

/// `baml test`'s default cap on concurrently running test bodies: twice the
/// core count. The runner admits a leaf only when a slot is free, so a
/// five-thousand-test corpus holds ~2N live VM threads instead of five
/// thousand, which keeps per-thread GC costs bounded and keeps wall-clock
/// timing assertions meaningful under full-corpus load.
pub fn default_max_concurrency() -> i64 {
    i64::try_from(std::thread::available_parallelism().map_or(8, std::num::NonZero::get) * 2)
        .unwrap_or(16)
}

/// A per-test deadline as `run_filtered` takes it. The stdlib treats a
/// non-positive timeout as "no deadline".
pub fn test_timeout_ms(timeout: Option<Duration>) -> i64 {
    timeout.map_or(0, |limit| {
        i64::try_from(limit.as_millis()).unwrap_or(i64::MAX).max(1)
    })
}

/// Run the selected tests: one BAML call expands, filters, runs
/// (concurrently via spawn/await) and aggregates, honoring testset runners,
/// and returns a `FlatTestReport` for [`parse_flat_report`].
pub async fn run_filtered(
    engine: &Arc<BexEngine>,
    registry: &BexExternalValue,
    selection: &TestSelection,
    max_concurrency: i64,
    test_timeout_ms: i64,
    call_context: FunctionCallContext,
) -> Result<BexExternalValue> {
    let mut args = vec![registry.clone()];
    args.extend(selection.args());
    args.push(BexExternalValue::Int(max_concurrency));
    args.push(BexExternalValue::Int(test_timeout_ms));
    engine
        .call_function(
            "testing.TestRegistry.run_filtered",
            args,
            call_context,
            true,
        )
        .await
        .map_err(|e| anyhow!("run_filtered failed: {e}"))
}

/// The canonical IDs the selection picks, without running anything.
pub async fn list_filtered(
    engine: &Arc<BexEngine>,
    registry: &BexExternalValue,
    selection: &TestSelection,
    call_context: FunctionCallContext,
) -> Result<Vec<String>> {
    let mut args = vec![registry.clone()];
    args.extend(selection.args());
    let value = engine
        .call_function(
            "testing.TestRegistry.list_filtered",
            args,
            call_context,
            true,
        )
        .await
        .map_err(|e| anyhow!("list_filtered failed: {e}"))?;
    Ok(string_array_values(&value))
}

// ---------------------------------------------------------------------------
// Flat report parsing
//
// `FlatTestReport` (baml_std/testing/types.baml) is intentionally shallow:
// primitive fields only, already aggregated (including the tolerated-failure
// split), so the driver reads it directly instead of walking a nested,
// union-wrapped `TestReport | TestSetReport` tree.
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct FlatReport {
    pub outcome: String,
    pub passed: usize,
    pub failed: usize,
    pub tolerated: usize,
    pub total: usize,
    pub passed_names: Vec<String>,
    pub failed_names: Vec<String>,
    pub tolerated_names: Vec<String>,
    /// Per-leaf durations parallel to the matching name arrays; may be
    /// shorter (or empty) when identities came from the registry's fallback
    /// correlation, and `-1` marks a leaf with no measured run — print the
    /// duration only when a non-negative entry exists at the name's index.
    pub passed_ms: Vec<i64>,
    pub failed_ms: Vec<i64>,
    pub tolerated_ms: Vec<i64>,
    /// The failure message of every failing run, in report order. Not keyed
    /// by leaf: a leaf with retries contributes one per failing run.
    pub messages: Vec<String>,
}

pub fn parse_flat_report(value: &BexExternalValue) -> Option<FlatReport> {
    let BexExternalValue::Instance { fields, .. } = unwrap_union(value) else {
        return None;
    };
    let outcome = fields
        .get("outcome")
        .and_then(|v| as_string(unwrap_union(v)))?
        .to_string();
    // Counts are always present in a well-formed FlatTestReport; treat a missing
    // one as an FFI contract break (parse fails -> reported, not silently zeroed).
    let passed = fields.get("passed").and_then(as_usize)?;
    let failed = fields.get("failed").and_then(as_usize)?;
    let tolerated = fields.get("tolerated").and_then(as_usize)?;
    let total = fields.get("total").and_then(as_usize)?;
    let strings = |name: &str| {
        fields
            .get(name)
            .map(string_array_values)
            .unwrap_or_default()
    };
    let ints = |name: &str| fields.get(name).map(int_array_values).unwrap_or_default();
    Some(FlatReport {
        outcome,
        passed,
        failed,
        tolerated,
        total,
        passed_names: strings("passed_names"),
        failed_names: strings("failed_names"),
        tolerated_names: strings("tolerated_names"),
        passed_ms: ints("passed_ms"),
        failed_ms: ints("failed_ms"),
        tolerated_ms: ints("tolerated_ms"),
        messages: strings("messages"),
    })
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Print a flat report the way `baml test` does and fold its counts into the
/// running totals.
///
/// Both filtered and unfiltered runs go through the same aggregate path
/// (filtered selections still run under their testset runners), so a failing
/// leaf whose runner still passes the suite is reported as a *tolerated* failure
/// — kept out of the hard `failed` count and not failing the command. Every
/// leaf line uses the same canonical ID emitted by `baml test --list`; aggregate
/// runner verdicts are labeled as aggregates rather than presented as test IDs.
pub fn consume_flat_report(
    flat: &FlatReport,
    passed: &mut usize,
    failed: &mut usize,
    tolerated: &mut usize,
    total: &mut usize,
    command_failed: &mut bool,
) {
    *passed += flat.passed;
    *failed += flat.failed;
    *tolerated += flat.tolerated;
    *total += flat.total;

    // An empty selection (a filter that matched no testset tests) aggregates
    // to a vacuous `pass` over zero tests. Printing a green aggregate pass
    // for that contradicts the `NoTestsRun` exit the caller then returns and
    // reads as success to anything parsing stdout, so skip the aggregate line
    // entirely and let the caller's "no tests selected" guard speak.
    //
    // Guard *only* the vacuous-pass case: a zero-test report with a non-pass
    // outcome must still fall through to the FAIL branch so it prints, sets
    // `command_failed`, and synthesizes a displayed failure — a real failure
    // is never silently dropped just because it carried no leaves.
    if flat.is_vacuous_pass() {
        return;
    }

    // Duration suffixes come from the parallel `*_ms` arrays; a missing or
    // negative entry (fallback-correlated identities, expansion sentinels)
    // prints the bare line the output always had.
    let with_ms = |ms: &[i64], i: usize| -> String {
        match ms.get(i) {
            Some(ms) if *ms >= 0 => format!(" ({ms}ms)"),
            _ => String::new(),
        }
    };
    for (i, name) in flat.passed_names.iter().enumerate() {
        println!("PASS {name}{}", with_ms(&flat.passed_ms, i));
    }
    for (i, name) in flat.tolerated_names.iter().enumerate() {
        println!("TOLERATED {name}{}", with_ms(&flat.tolerated_ms, i));
    }
    for (i, name) in flat.failed_names.iter().enumerate() {
        println!("FAIL {name}{}", with_ms(&flat.failed_ms, i));
    }

    if flat.outcome == "pass" {
        if flat.tolerated > 0 {
            println!(
                "AGGREGATE PASS [outcome=pass; {} tolerated {}]",
                flat.tolerated,
                pluralize(flat.tolerated, "failure", "failures")
            );
        }
    } else {
        println!("AGGREGATE FAIL [outcome={}]", flat.outcome);
        for message in &flat.messages {
            eprintln!("  => {message}");
        }
        // A testset runner can fail the aggregate without marking any child
        // failed. Count that verdict as one displayed failure so the summary
        // does not read "0 failed" when the aggregate itself failed.
        if flat.failed == 0 {
            *failed += 1;
            *total += 1;
        }
        *command_failed = true;
    }
}

impl FlatReport {
    /// A selection that matched nothing: zero leaves, and nothing failed.
    pub fn is_vacuous_pass(&self) -> bool {
        self.total == 0 && self.outcome == "pass"
    }

    /// Whether the run passed: the aggregate outcome is `pass`.
    pub fn passed(&self) -> bool {
        self.outcome == "pass"
    }

    /// The report as text that two runs of the same tests print
    /// identically — for comparing an interpreted run against a native one
    /// byte for byte: one line per leaf (`PASS`, `TOLERATED` or `FAIL`), in
    /// name order; then every failure message, as `baml test` prints it but
    /// sorted (the report does not key messages by leaf, and leaves finish
    /// in whatever order the pool ran them); then one `AGGREGATE` line
    /// carrying the counts. No durations anywhere.
    ///
    /// `extra_failures` are failures the run surfaced outside the registry —
    /// an unhandled spawned task, which `baml test` reports as
    /// `testing::unhandled_spawn_error` — as `(name, message)`; each counts
    /// as one failed leaf and fails the aggregate.
    pub fn stable_text(&self, extra_failures: &[(String, String)]) -> String {
        let mut leaves: Vec<(&str, &str)> = self
            .passed_names
            .iter()
            .map(|name| (name.as_str(), "PASS"))
            .chain(
                self.tolerated_names
                    .iter()
                    .map(|name| (name.as_str(), "TOLERATED")),
            )
            .chain(self.failed_names.iter().map(|name| (name.as_str(), "FAIL")))
            .chain(
                extra_failures
                    .iter()
                    .map(|(name, _)| (name.as_str(), "FAIL")),
            )
            .collect();
        leaves.sort_unstable();
        let mut messages: Vec<&str> = self
            .messages
            .iter()
            .map(String::as_str)
            .chain(extra_failures.iter().map(|(_, message)| message.as_str()))
            .collect();
        messages.sort_unstable();

        let mut out = String::new();
        for (name, verdict) in leaves {
            out.push_str(verdict);
            out.push(' ');
            out.push_str(name);
            out.push('\n');
        }
        for message in messages {
            out.push_str("  => ");
            out.push_str(message);
            out.push('\n');
        }
        let failed = self.failed + extra_failures.len();
        let total = self.total + extra_failures.len();
        let verdict = if self.passed() && extra_failures.is_empty() {
            "PASS"
        } else {
            "FAIL"
        };
        let _ = writeln!(
            out,
            "AGGREGATE {verdict} [outcome={}] {} passed, {failed} failed, {} tolerated, {total} total",
            self.outcome, self.passed, self.tolerated
        );
        out
    }
}

pub fn pluralize(n: usize, singular: &str, plural: &str) -> String {
    if n == 1 {
        singular.to_string()
    } else {
        plural.to_string()
    }
}

// ---------------------------------------------------------------------------
// FFI value helpers
// ---------------------------------------------------------------------------

pub fn string_array(items: &[String]) -> BexExternalValue {
    BexExternalValue::Array {
        element_type: WireTy::string(),
        items: items
            .iter()
            .map(|s| BexExternalValue::String(s.as_str().into()))
            .collect(),
    }
}

fn int_array_values(value: &BexExternalValue) -> Vec<i64> {
    match unwrap_union(value) {
        BexExternalValue::Array { items, .. } => items
            .iter()
            .filter_map(|item| match unwrap_union(item) {
                BexExternalValue::Int(i) => Some(*i),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub fn string_array_values(value: &BexExternalValue) -> Vec<String> {
    match unwrap_union(value) {
        BexExternalValue::Array { items, .. } => items
            .iter()
            .filter_map(|item| as_string(unwrap_union(item)).map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// Strip a leading `Union { value, … }` wrapper. Union-typed values (e.g. the
/// `Outcome = "pass" | "fail" | "error"` field) come back wrapped when
/// deep-copied across the FFI boundary.
fn unwrap_union(value: &BexExternalValue) -> &BexExternalValue {
    match value {
        BexExternalValue::Union { value, .. } => unwrap_union(value),
        other => other,
    }
}

fn as_string(value: &BexExternalValue) -> Option<&str> {
    match value {
        BexExternalValue::String(s) => Some(s.as_str()),
        BexExternalValue::Union { value, .. } => as_string(value),
        _ => None,
    }
}

fn as_usize(value: &BexExternalValue) -> Option<usize> {
    match unwrap_union(value) {
        BexExternalValue::Int(i) => usize::try_from(*i).ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string(value: &str) -> BexExternalValue {
        BexExternalValue::String(value.into())
    }

    fn int(value: i64) -> BexExternalValue {
        BexExternalValue::Int(value)
    }

    fn array(items: Vec<BexExternalValue>) -> BexExternalValue {
        BexExternalValue::Array {
            element_type: WireTy::unknown(),
            items,
        }
    }

    fn instance(class_name: &str, fields: Vec<(&str, BexExternalValue)>) -> BexExternalValue {
        BexExternalValue::Instance {
            class_name: class_name.to_string(),
            type_args: Vec::new(),
            fields: fields
                .into_iter()
                .map(|(name, value)| (name.to_string(), value))
                .collect(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn flat_report(
        outcome: &str,
        passed: i64,
        failed: i64,
        tolerated: i64,
        total: i64,
        failed_names: Vec<BexExternalValue>,
        messages: Vec<BexExternalValue>,
    ) -> BexExternalValue {
        instance(
            "testing.FlatTestReport",
            vec![
                ("outcome", string(outcome)),
                ("passed", int(passed)),
                ("failed", int(failed)),
                ("tolerated", int(tolerated)),
                ("total", int(total)),
                ("passed_names", array(Vec::new())),
                ("failed_names", array(failed_names)),
                ("tolerated_names", array(Vec::new())),
                ("messages", array(messages)),
            ],
        )
    }

    #[test]
    fn parses_flat_report_fields() {
        let value = flat_report(
            "fail",
            1,
            1,
            1,
            3,
            vec![string("hard/fails")],
            vec![string("assertion failed")],
        );

        let parsed = parse_flat_report(&value).expect("should parse");
        assert_eq!(parsed.outcome, "fail");
        assert_eq!(
            (parsed.passed, parsed.failed, parsed.tolerated, parsed.total),
            (1, 1, 1, 3)
        );
        assert_eq!(parsed.failed_names, vec!["hard/fails".to_string()]);
        assert_eq!(parsed.messages, vec!["assertion failed".to_string()]);
    }

    #[test]
    fn parse_flat_report_unwraps_union_wrapped_outcome() {
        // Union-typed fields (Outcome) arrive wrapped across the FFI boundary.
        let wrapped_outcome = BexExternalValue::union(
            string("pass"),
            [WireTy::string(), WireTy::int()],
            WireTy::string(),
        );
        let value = instance(
            "testing.FlatTestReport",
            vec![
                ("outcome", wrapped_outcome),
                ("passed", int(1)),
                ("failed", int(0)),
                ("tolerated", int(0)),
                ("total", int(1)),
                ("passed_names", array(vec![string("root::passes")])),
                ("failed_names", array(Vec::new())),
                ("tolerated_names", array(Vec::new())),
                ("messages", array(Vec::new())),
            ],
        );

        let parsed = parse_flat_report(&value).expect("should parse");
        assert_eq!(parsed.outcome, "pass");
        assert_eq!(parsed.total, 1);
        assert_eq!(parsed.passed_names, ["root::passes"]);
    }

    fn consume(report: &FlatReport) -> (usize, usize, usize, usize, bool) {
        let (mut passed, mut failed, mut tolerated, mut total, mut command_failed) =
            (0, 0, 0, 0, false);
        consume_flat_report(
            report,
            &mut passed,
            &mut failed,
            &mut tolerated,
            &mut total,
            &mut command_failed,
        );
        (passed, failed, tolerated, total, command_failed)
    }

    #[test]
    fn consume_empty_report_folds_zero_counts_without_printing_pass() {
        // A no-match selector aggregates to a vacuous `pass` over zero tests.
        // Folding it must leave every counter at zero (so the caller's
        // `total == 0` guard fires "no tests selected") and must NOT print a
        // green aggregate pass line contradicting the non-zero exit (B-628).
        let parsed =
            parse_flat_report(&flat_report("pass", 0, 0, 0, 0, Vec::new(), Vec::new())).unwrap();
        assert_eq!(consume(&parsed), (0, 0, 0, 0, false));
    }

    #[test]
    fn consume_empty_report_with_fail_outcome_still_propagates_failure() {
        // The vacuous-pass skip must NOT swallow a zero-test report that
        // reports a failure. A `total == 0 && outcome == "fail"` report has to
        // fall through to the FAIL branch: it sets `command_failed` and
        // synthesizes one displayed failure (so the summary isn't "0 failed"
        // while the aggregate failed). Regression guard for the narrowed guard.
        let parsed =
            parse_flat_report(&flat_report("fail", 0, 0, 0, 0, Vec::new(), Vec::new())).unwrap();
        assert_eq!(consume(&parsed), (0, 1, 0, 1, true));
    }

    #[test]
    fn consume_aggregate_pass_with_tolerated_failure() {
        // PassRate-style: aggregate passes; the failing leaf is tolerated, not hard.
        let parsed =
            parse_flat_report(&flat_report("pass", 2, 0, 1, 3, Vec::new(), Vec::new())).unwrap();
        assert_eq!(consume(&parsed), (2, 0, 1, 3, false));
    }

    #[test]
    fn consume_aggregate_fail_keeps_tolerated_out_of_failed() {
        // A hard sibling fails the command; tolerated stays separate from failed.
        let parsed = parse_flat_report(&flat_report(
            "fail",
            1,
            1,
            1,
            3,
            vec![string("hard/fails")],
            Vec::new(),
        ))
        .unwrap();
        assert_eq!(consume(&parsed), (1, 1, 1, 3, true));
    }

    #[test]
    fn consume_fail_with_zero_hard_failed_synthesizes_one() {
        // Runner fails the aggregate without marking any child failed.
        let parsed =
            parse_flat_report(&flat_report("fail", 1, 0, 0, 1, Vec::new(), Vec::new())).unwrap();
        assert_eq!(consume(&parsed), (1, 1, 0, 2, true));
    }

    fn report(outcome: &str) -> FlatReport {
        FlatReport {
            outcome: outcome.into(),
            passed: 2,
            failed: 1,
            tolerated: 1,
            total: 4,
            // Names arrive in pool-completion order, with durations that
            // differ run to run.
            passed_names: vec!["root::b::two".into(), "root::a::one".into()],
            failed_names: vec!["root::a::zero".into()],
            tolerated_names: vec!["root::c::flaky".into()],
            passed_ms: vec![12, 3],
            failed_ms: vec![7],
            tolerated_ms: vec![-1],
            messages: vec![
                "assertion failed: left = 1, right = 2".into(),
                "assertion failed: expected true".into(),
            ],
        }
    }

    /// The stable text sorts leaves by name and messages by content, and
    /// carries no durations, so two runs of the same tests print the same.
    #[test]
    fn stable_text_is_sorted_and_timing_free() {
        assert_eq!(
            report("fail").stable_text(&[]),
            "PASS root::a::one\n\
             FAIL root::a::zero\n\
             PASS root::b::two\n\
             TOLERATED root::c::flaky\n\
             \x20 => assertion failed: expected true\n\
             \x20 => assertion failed: left = 1, right = 2\n\
             AGGREGATE FAIL [outcome=fail] 2 passed, 1 failed, 1 tolerated, 4 total\n"
        );
    }

    /// An unhandled spawned-task failure is one more failed leaf, under the
    /// name `baml test` gives it, and fails an aggregate that passed.
    #[test]
    fn stable_text_counts_extra_failures() {
        let mut passing = report("pass");
        passing.failed = 0;
        passing.failed_names.clear();
        passing.total = 3;
        passing.messages.clear();
        assert_eq!(
            passing.stable_text(&[(
                "testing::unhandled_spawn_error".into(),
                "task failed".into()
            )]),
            "PASS root::a::one\n\
             PASS root::b::two\n\
             TOLERATED root::c::flaky\n\
             FAIL testing::unhandled_spawn_error\n\
             \x20 => task failed\n\
             AGGREGATE FAIL [outcome=pass] 2 passed, 1 failed, 1 tolerated, 4 total\n"
        );
        assert_eq!(
            passing.stable_text(&[]),
            "PASS root::a::one\n\
             PASS root::b::two\n\
             TOLERATED root::c::flaky\n\
             AGGREGATE PASS [outcome=pass] 2 passed, 0 failed, 1 tolerated, 3 total\n"
        );
    }

    #[test]
    fn test_timeout_ms_follows_the_stdlib_contract() {
        assert_eq!(test_timeout_ms(None), 0);
        assert_eq!(test_timeout_ms(Some(Duration::from_millis(0))), 1);
        assert_eq!(test_timeout_ms(Some(Duration::from_secs(2))), 2000);
    }
}

/// Execute one canonical ID without treating literal `*` or a shorter test name
/// as a selector. Uses the same stdlib selection/runner/flattening path as test.
pub async fn run_exact(
    engine: &Arc<BexEngine>,
    registry: &BexExternalValue,
    selection: &TestSelection,
    name: &str,
    call_context: FunctionCallContext,
) -> Result<BexExternalValue> {
    let cancel = call_context.cancel.clone();
    let logger = call_context.logger.clone();
    let auxiliary_context = || {
        bex_engine::FunctionCallContextBuilder::new(bex_engine::CallId::next())
            .with_cancel_token(cancel.clone())
            .with_logger(logger.clone())
            .build()
    };
    let mut selection = selection.clone();
    selection.cli_include = vec![name.to_owned()];
    let available = list_filtered(engine, registry, &selection, auxiliary_context()).await?;
    if !available.iter().any(|candidate| candidate == name) {
        return Err(anyhow!(
            "exact test is not selected by the active profile: {name}"
        ));
    }
    let names = string_array(&[name.to_owned()]);
    let report = engine
        .call_function(
            "testing.TestRegistry._run_selected",
            vec![registry.clone(), names.clone()],
            call_context,
            true,
        )
        .await
        .map_err(|e| anyhow!("exact test execution failed: {e}"))?;
    engine
        .call_function(
            "testing._flatten_with_tolerated",
            vec![report_host_value(report), names],
            auxiliary_context(),
            true,
        )
        .await
        .map_err(|e| anyhow!("exact test report failed: {e}"))
}

// Outbound union envelopes annotate a declared return type; inbound values state
// their own type. Preserve the actual nodes when returning the report to BAML.
fn report_host_value(value: BexExternalValue) -> BexExternalValue {
    match value {
        BexExternalValue::Union { value, .. } => report_host_value(*value),
        BexExternalValue::Array {
            element_type,
            items,
        } => BexExternalValue::Array {
            element_type,
            items: items.into_iter().map(report_host_value).collect(),
        },
        BexExternalValue::Instance {
            class_name,
            type_args,
            fields,
        } => BexExternalValue::Instance {
            class_name,
            type_args,
            fields: fields
                .into_iter()
                .map(|(k, v)| (k, report_host_value(v)))
                .collect(),
        },
        other => other,
    }
}
