//! Decoding JSON text allocates each string once: a flat cell copied straight
//! from the parser's bytes, with nothing in between. Pinned by counting heap
//! cells across a parse of a 1 MB array of 10k strings against a parse of an
//! array of as many `null`s, which builds the same list and nothing else.

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue as Ext, FunctionCallContextBuilder};
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
function Parse(text: string) -> json { baml.json.parse(text) }
function Decode(text: string) -> string[] { baml.json.decode<string[]>(text) }
function DecodeNulls(text: string) -> null[] { baml.json.decode<null[]>(text) }
function Count(values: string[]) -> int { values.length() }
"#;

const STRINGS: usize = 10_000;

fn engine() -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new(
            baml_test_support::compile_source(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            vec![],
        )
        .unwrap(),
    )
}

async fn call(engine: &Arc<BexEngine>, name: &str, args: Vec<Ext>) -> Ext {
    engine
        .call_function(
            name,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            false,
        )
        .await
        .unwrap()
}

/// A JSON array of `STRINGS` distinct strings, about 1 MB of text.
fn strings_text() -> String {
    let items: Vec<String> = (0..STRINGS)
        .map(|i| format!("\"{i:05}-{}\"", "abcdefghij".repeat(10)))
        .collect();
    format!("[{}]", items.join(","))
}

/// A JSON array of `STRINGS` nulls: the same list, no string cells.
fn nulls_text() -> String {
    format!("[{}]", vec!["null"; STRINGS].join(","))
}

/// Cells `strings` allocates reading the strings array, beyond what `nulls`
/// allocates reading the nulls array: the string cells. Each call keeps its
/// result live, so no collection can run in between.
async fn cells_for_strings(engine: &Arc<BexEngine>, strings: &str, nulls: &str) -> usize {
    let collections = engine.heap().gc_budget().full_collections;
    let before = engine.heap_stats().runtime_objects;
    let nulls = call(engine, nulls, vec![Ext::String(nulls_text().into())]).await;
    let middle = engine.heap_stats().runtime_objects;
    let strings = call(engine, strings, vec![Ext::String(strings_text().into())]).await;
    let after = engine.heap_stats().runtime_objects;
    assert_eq!(
        engine.heap().gc_budget().full_collections,
        collections,
        "no collection ran during the parses"
    );
    drop((nulls, strings));
    (after - middle) - (middle - before)
}

#[tokio::test]
async fn parse_allocates_one_flat_cell_per_string() {
    let engine = engine();
    let text = strings_text();
    assert!(text.len() >= 1 << 20, "{} bytes", text.len());
    assert_eq!(cells_for_strings(&engine, "Parse", "Parse").await, STRINGS);
    engine.shutdown().await;
}

#[tokio::test]
async fn decode_allocates_one_flat_cell_per_string() {
    let engine = engine();
    assert_eq!(
        cells_for_strings(&engine, "Decode", "DecodeNulls").await,
        STRINGS
    );
    let decoded = call(&engine, "Decode", vec![Ext::String(strings_text().into())]).await;
    assert_eq!(
        call(&engine, "Count", vec![decoded]).await,
        Ext::Int(i64::try_from(STRINGS).unwrap())
    );
    engine.shutdown().await;
}
