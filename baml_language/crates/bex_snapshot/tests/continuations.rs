//! Native continuation frames in snapshots (format version 2).
//!
//! The callback of an array combinator, of a JSON walk, or of a `to_string`
//! walk runs under a native frame. Every program below yields inside such a
//! callback (a `println` sys-op, and with `yield_interval == 1` also early
//! yields), the harness moves the run to a fresh heap and VM at each of those
//! yields, and the result must equal the result of a plain run.

#![cfg(not(target_arch = "wasm32"))]

mod common;

use std::sync::atomic::Ordering;

use bex_heap::CollectionLevel;
use common::Harness;

const COMBINATORS: &str = r#"
class Item {
    name string
    weight int
}

function say(text: string) -> int {
    baml.io.println(text);
    0
}

function main() -> string {
    let items = [
        Item { name: "c", weight: 3 },
        Item { name: "a", weight: 1 },
        Item { name: "b", weight: 2 },
    ];
    let names = items.map((item) -> { say("map"); item.name });
    let heavy = items.filter((item) -> { say("filter"); item.weight > 1 });
    let total = items.reduce((sum, item) -> { say("reduce"); sum + item.weight }, 0);
    let found = items.find((item) -> { say("find"); item.weight == 1 });
    let last = items.find_last((item) -> { say("find_last"); item.weight < 3 });
    let some = items.some((item) -> { say("some"); item.weight == 2 });
    let every = items.every((item) -> { say("every"); item.weight > 0 });
    let flat = items.flat_map((item) -> { say("flat_map"); [item.name, item.name + "!"] });
    let sorted = [5, 3, 9, 1, 7].sort_by((a: int, b: int) -> baml.ops.Ordering throws never {
        say("sort_by");
        a.cmp(b)
    });
    let nested = [[1, 2], [3]].map((row) -> {
        row.map((n) -> { say("inner"); n * 10 })
    });
    names.to_string() + " " + heavy.length().to_string() + " " + total.to_string()
        + " " + (found?.name ?? "none") + " " + (last?.name ?? "none")
        + " " + some.to_string() + " " + every.to_string() + " " + flat.to_string()
        + " " + sorted.to_string() + " " + nested.to_string()
}
"#;

const WALKS: &str = r#"
function say(text: string) -> int {
    {
        baml.io.println(text);
        0
    } catch (e) {
        baml.errors.Io => 1
    }
}

class Secret {
    value string

    implements baml.ToString {
        function to_string(self) -> string throws never {
            say("to_string of " + self.value);
            "<secret " + self.value.length().to_string() + ">"
        }
    }

    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            say("to_json of " + self.value);
            "hidden " + self.value.length().to_string()
        }
    }
}

class Vault {
    label string
    secrets Secret[]
    spare Secret?
}

function main() -> string throws baml.json.SerializationError {
    let vault = Vault {
        label: "v",
        secrets: [Secret { value: "one" }, Secret { value: "three" }],
        spare: Secret { value: "spare" },
    };
    let text = vault.to_string();
    let encoded = baml.json.stringify(vault.to_json());
    text + " | " + encoded
}
"#;

fn hops_match_a_plain_run(source: &str, min_hops: usize) {
    let harness = Harness::new(source);
    let expected = harness.run_plain("user.main");
    for (compress, level) in [
        (false, None),
        (true, Some(CollectionLevel::Major)),
        (false, Some(CollectionLevel::Minor)),
    ] {
        harness.gc_after_restore.set(level);
        let (result, hops) = harness.run_hopping("user.main", compress, 0);
        assert_eq!(result, expected, "compress={compress} gc={level:?}");
        assert!(hops >= min_hops, "only {hops} hops");
    }
}

#[test]
fn every_array_combinator_survives_a_hop_inside_its_callback() {
    // One hop per `say` call: 3 + 3 + 3 + 2 + 2 + 3 + 3 + 3 + comparisons + 3.
    hops_match_a_plain_run(COMBINATORS, 25);
}

#[test]
fn array_combinators_survive_early_yield_hops_inside_their_callbacks() {
    let harness = Harness::new(COMBINATORS);
    let expected = harness.run_plain("user.main");
    // Yield at every check, also right after a call and right after a return,
    // so that hops land between the instructions of the callbacks.
    harness.yield_interval.set(1);
    harness.park.store(true, Ordering::Release);
    harness.gc_after_restore.set(Some(CollectionLevel::Major));
    let (result, hops) = harness.run_hopping("user.main", false, 60);
    assert_eq!(result, expected);
    assert!(hops >= 60, "only {hops} hops");
}

#[test]
fn to_string_and_to_json_walks_survive_a_hop_inside_an_override() {
    hops_match_a_plain_run(WALKS, 6);
}
