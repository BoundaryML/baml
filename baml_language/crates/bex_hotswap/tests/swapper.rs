//! `Swapper`: what a process checks before a swap from Boundary Cloud
//! installs (BEP-087, stage 2). Every engine here runs as production does:
//! no `baml run --hot`, so only marked functions can change.

use std::{sync::Arc, time::Duration};

use bcs_api::hotswaps::{Swap, SwapFunction};
use bex_engine::{BexEngine, BexExternalValue, CallId, EngineConfig, FunctionCallContextBuilder};
use bex_hotswap::{HotswapBuild, HotswapConfig, Swapper};
use sys_native::SysOpsExt as _;

const SOURCE: &str = r#"
function helper(x: int) -> int { x + 1 }

function other(x: int) -> int { x * 100 }

/// baml:hotswap=hotswap.any
function step(x: int) -> int { helper(x) }

/// baml:hotswap=hotswap.no_new_calls
function guarded(x: int) -> int { helper(x) }

/// baml:hotswap=hotswap.select_only([double, triple])
function route(x: int) -> int { x }

function double(x: int) -> int { x * 2 }

function triple(x: int) -> int { x * 3 }

/// baml:hotswap=hotswap.telemetry_only
function traced(x: int) -> int { helper(x) }

function unmarked(x: int) -> int { x }
"#;

const COMMIT: &str = "3f2a91c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6";

fn build(config: HotswapConfig) -> Arc<HotswapBuild> {
    Arc::new(HotswapBuild {
        sources: [("main.baml".to_string(), SOURCE.to_string())].into(),
        commit: Some(COMMIT.to_string()),
        config,
    })
}

fn swapper_with(config: HotswapConfig) -> (Arc<BexEngine>, Swapper) {
    let build = build(config);
    let program = baml_test_support::compile_multi_file(&[("main.baml", SOURCE)]);
    let engine = Arc::new(
        BexEngine::new_with_config(
            program,
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            EngineConfig {
                runtime_compiler: Some(bex_project::runtime_compiler()),
                hotswap_baseline: Some(build.baseline()),
                hotswap_marked: Some(build.marked()),
                ..EngineConfig::default()
            },
        )
        .expect("engine"),
    );
    let swapper = Swapper::new(Arc::clone(&engine), build);
    (engine, swapper)
}

fn swapper() -> (Arc<BexEngine>, Swapper) {
    swapper_with(HotswapConfig::default())
}

async fn int(engine: &Arc<BexEngine>, name: &str, arg: i64) -> i64 {
    let context = FunctionCallContextBuilder::new(CallId::next()).build();
    match engine
        .call_function(name, vec![BexExternalValue::Int(arg)], context, true)
        .await
        .unwrap_or_else(|error| panic!("{name}: {error}"))
    {
        BexExternalValue::Int(value) => value,
        other => panic!("{name} returned {other:?}"),
    }
}

/// The declaration of `name` in `SOURCE`, its leading mark included.
fn declaration(name: &str) -> String {
    let start = SOURCE.find(&format!("function {name}(")).expect("declared");
    let start = SOURCE[..start]
        .rfind("\n\n")
        .map_or(start, |blank| blank + 2);
    let end = start + SOURCE[start..].find('}').expect("closed") + 1;
    SOURCE[start..end].to_string()
}

/// A swap of `name` from its declaration to `edit` of it.
fn swap(id: &str, edits: &[(&str, &str, &str)]) -> Swap {
    Swap {
        id: id.into(),
        revision: 1,
        baseline: COMMIT[..7].into(),
        replaces: None,
        published_by: Some("alice".into()),
        approved_by: Some("bob".into()),
        functions: edits
            .iter()
            .map(|(name, from, to)| {
                let old_source = declaration(name);
                assert!(old_source.contains(from), "{from:?} not in {old_source:?}");
                SwapFunction {
                    name: (*name).into(),
                    file: "main.baml".into(),
                    rule: String::new(),
                    old_source: old_source.clone(),
                    new_source: old_source.replacen(from, to, 1),
                }
            })
            .collect(),
    }
}

fn rejected(result: Result<impl std::fmt::Debug, bex_hotswap::SwapperError>, needle: &str) {
    match result {
        Err(error) => assert!(error.0.contains(needle), "{error}"),
        Ok(value) => panic!("expected a rejection naming {needle:?}, got {value:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_marked_function_swaps_and_returns_to_the_build_when_removed() {
    let (engine, mut swapper) = swapper();
    assert_eq!(int(&engine, "step", 1).await, 2);
    let changed = swapper
        .install(&swap("hs_1", &[("step", "helper(x)", "helper(x) * 10")]))
        .await
        .unwrap();
    assert_eq!(changed.into_iter().collect::<Vec<_>>(), ["step"]);
    assert_eq!(int(&engine, "step", 1).await, 20);
    assert_eq!(swapper.installed().len(), 1);

    // The replacement's spans name its swap.
    let metadata = engine.program_metadata().await;
    assert!(
        metadata
            .function_table
            .functions
            .iter()
            .any(|function| function.hotswap_id.as_deref() == Some("hs_1")),
        "no function records hs_1"
    );

    swapper.remove("hs_1", "deleted").await.unwrap();
    assert_eq!(int(&engine, "step", 1).await, 2);
    assert!(swapper.installed().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn only_marked_functions_change() {
    let (engine, mut swapper) = swapper();
    rejected(
        swapper
            .install(&swap("hs_1", &[("unmarked", "{ x }", "{ x + 1 }")]))
            .await,
        "`unmarked` is not marked `/// baml:hotswap`",
    );
    assert_eq!(int(&engine, "unmarked", 1).await, 1);
    // Nor can a swap drop or change a mark.
    rejected(
        swapper
            .install(&swap(
                "hs_2",
                &[("step", "/// baml:hotswap=hotswap.any\n", "")],
            ))
            .await,
        "cannot change its `/// baml:hotswap` mark",
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn each_rule_accepts_and_rejects_in_the_process() {
    let (engine, mut swapper) = swapper();
    // no_new_calls
    swapper
        .install(&swap("hs_1", &[("guarded", "helper(x)", "helper(x) + 5")]))
        .await
        .unwrap();
    assert_eq!(int(&engine, "guarded", 1).await, 7);
    rejected(
        swapper
            .install(&swap("hs_2", &[("route", "{ x }", "{ other(x) }")]))
            .await,
        "route: hotswap.select_only",
    );
    let (engine, mut swapper) = swapper_fresh();
    rejected(
        swapper
            .install(&swap("hs_3", &[("guarded", "helper(x)", "other(x)")]))
            .await,
        "guarded: hotswap.no_new_calls: the replacement calls `other`, which the original does not",
    );
    // select_only takes a shipped function's code, not an edited copy.
    swapper
        .install(&swap("hs_4", &[("route", "{ x }", "{ x * 3 }")]))
        .await
        .unwrap();
    assert_eq!(int(&engine, "route", 2).await, 6);
    rejected(
        swapper
            .install(&swap("hs_5", &[("traced", "helper(x)", "helper(x) + 1")]))
            .await,
        "traced: hotswap.telemetry_only: the change does more than add telemetry",
    );
    swapper
        .install(&swap(
            "hs_6",
            &[(
                "traced",
                "{ helper(x) }",
                "{\n  log.info(`x is ${x}`)\n  helper(x)\n}",
            )],
        ))
        .await
        .unwrap();
    assert_eq!(int(&engine, "traced", 1).await, 2);
}

fn swapper_fresh() -> (Arc<BexEngine>, Swapper) {
    swapper()
}

#[tokio::test(flavor = "multi_thread")]
async fn one_swap_per_function_unless_it_replaces_the_other() {
    let (engine, mut swapper) = swapper();
    swapper
        .install(&swap("hs_a", &[("step", "helper(x)", "helper(x) * 10")]))
        .await
        .unwrap();
    // Another function is independent.
    swapper
        .install(&swap("hs_b", &[("guarded", "helper(x)", "helper(x) + 5")]))
        .await
        .unwrap();
    rejected(
        swapper
            .install(&swap("hs_c", &[("step", "helper(x)", "helper(x) * 20")]))
            .await,
        "`step` already has swap hs_a",
    );
    assert_eq!(int(&engine, "step", 1).await, 20);

    let mut replacement = swap("hs_c", &[("step", "helper(x)", "helper(x) * 20")]);
    replacement.replaces = Some("hs_a".into());
    swapper.install(&replacement).await.unwrap();
    assert_eq!(int(&engine, "step", 1).await, 40);
    assert_eq!(int(&engine, "guarded", 1).await, 7);
    let mut ids: Vec<String> = swapper.installed().into_iter().map(|s| s.id).collect();
    ids.sort();
    assert_eq!(ids, ["hs_b", "hs_c"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_swap_installs_only_on_its_baseline_and_with_approval() {
    let (_, mut swapper) = swapper();
    let mut other_commit = swap("hs_1", &[("step", "helper(x)", "helper(x) * 10")]);
    other_commit.baseline = "aaaaaaa".into();
    rejected(
        swapper.install(&other_commit).await,
        "the process was built from 3f2a91c, not the baseline aaaaaaa",
    );
    let mut unapproved = swap("hs_2", &[("step", "helper(x)", "helper(x) * 10")]);
    unapproved.approved_by = None;
    rejected(swapper.install(&unapproved).await, "was not approved");

    // A build that does not require approval takes it.
    let (engine, mut swapper) = swapper_with(HotswapConfig {
        require_approval: false,
        ..HotswapConfig::default()
    });
    swapper.install(&unapproved).await.unwrap();
    assert_eq!(int(&engine, "step", 1).await, 20);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_swap_without_renewal_lapses() {
    let (engine, swapper) = swapper();
    let mut swapper = swapper.with_lapse(Duration::from_millis(50));
    swapper
        .install(&swap("hs_1", &[("step", "helper(x)", "helper(x) * 10")]))
        .await
        .unwrap();
    assert!(swapper.lapsed().is_empty());
    tokio::time::sleep(Duration::from_millis(30)).await;
    swapper.renew("hs_1");
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(swapper.lapsed().is_empty(), "renewed 30ms ago");
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert_eq!(swapper.lapsed(), ["hs_1"]);
    swapper.remove("hs_1", "lapsed").await.unwrap();
    assert_eq!(int(&engine, "step", 1).await, 2);
}

#[test]
fn marks_without_a_rule_need_a_default_rule() {
    let unruled = HotswapBuild {
        sources: [(
            "main.baml".to_string(),
            "/// baml:hotswap\nfunction f() -> int { 1 }\n".to_string(),
        )]
        .into(),
        commit: None,
        config: HotswapConfig::default(),
    };
    assert_eq!(
        unruled.check_marks(),
        Err("`f` is marked `/// baml:hotswap` with no rule, and baml.toml sets no `[hotswap] default_rule`".to_string())
    );
    assert_eq!(unruled.marked().into_iter().collect::<Vec<_>>(), ["f"]);
    let defaulted = HotswapBuild {
        config: HotswapConfig {
            default_rule: Some(bex_project::hotswap_rules::Rule::Any),
            ..HotswapConfig::default()
        },
        ..unruled
    };
    assert_eq!(defaulted.check_marks(), Ok(()));
}

/// A replacement retires the swap it replaces here, though BCS keeps that one
/// live until the replacement installs everywhere; a rejected replacement
/// leaves the swap it replaces running.
#[tokio::test(flavor = "multi_thread")]
async fn a_rejected_replacement_leaves_the_swap_it_replaces() {
    let (engine, mut swapper) = swapper();
    swapper
        .install(&swap("hs_a", &[("guarded", "helper(x)", "helper(x) + 5")]))
        .await
        .unwrap();
    let mut bad = swap("hs_b", &[("guarded", "helper(x)", "other(x)")]);
    bad.replaces = Some("hs_a".into());
    rejected(swapper.install(&bad).await, "hotswap.no_new_calls");
    assert_eq!(int(&engine, "guarded", 1).await, 7);
    assert!(!swapper.is_retired("hs_a"));

    let mut good = swap("hs_c", &[("guarded", "helper(x)", "helper(x) + 6")]);
    good.replaces = Some("hs_a".into());
    swapper.install(&good).await.unwrap();
    assert_eq!(int(&engine, "guarded", 1).await, 8);
    assert!(swapper.is_retired("hs_a"));
}
