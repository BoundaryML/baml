//! Table editing and format-preserving serialization tests, plus the ignored
//! JSON conversion regression. The original parsing/conversion runtime corpus
//! lives in `baml_src/ns_toml/toml.baml`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// `Table.from_json` base path plus its documented lossy null-skip; transitively
/// covers `item_from_json`'s scalar and nested-map arms and the round-trip back
/// through `to_json`. The `"b": null` entry must be dropped.
///
/// Ignored: `from_json` skips nulls with `continue` inside the key loop, and
/// `continue`/`break` are currently broken in the VM — the loop body surfaces a
/// spurious `TypeError { expected: Map, got: Any }` rather than skipping. Drop
/// the `#[ignore]` once that VM bug is fixed; the assertion below is the intended
/// behavior.
#[tokio::test]
#[ignore = "blocked on VM continue/break bug; toml.baml from_json skips nulls via `continue`"]
async fn from_json_skips_null() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let j: baml.json.json = baml.json.parse("{\"a\":1,\"b\":null,\"c\":{\"d\":\"x\"}}");
            baml.json.stringify(baml.toml.Table.from_json(j).to_json())
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            r#"{"a":1,"c":{"d":"x"}}"#.to_string().into()
        ))
    );
}

#[tokio::test]
async fn table_edits_preserve_source_and_accept_all_toml_types() {
    let output = baml_test!(
        r##"
        function main() -> string {
            let source = "# config\nname = 'app' # unchanged\ncount = 0x10 # workers\nratio = 1.0\nready = false\n[service] # settings\nport = 8080\n";
            let doc = baml.toml.Table.parse(source);
            doc.items["count"] = 32;
            doc.set("ratio", 2.5);
            doc.set("ready", true);
            let values: baml.toml.Item[] = ["a", 2, false];
            doc.set("values", values);
            doc.set("day", baml.time.PlainDate.parse("2026-10-03"));
            doc.set("time", baml.time.PlainTime.parse("12:30:00.123"));
            doc.set("local", baml.time.PlainDateTime.parse("2026-10-03T12:30:00"));
            doc.set("offset", baml.time.ZonedDateTime.parse("2026-10-03T12:30:00-07:00"));
            doc.table("service").set("port", 9000);
            let output = doc.to_string();
            let parsed = baml.toml.Table.parse(output);
            if (parsed.get("count") != 32 || parsed.table("service").get("port") != 9000) {
                baml.sys.panic("serialized edits did not roundtrip");
            }
            output
        }
    "##
    );
    let Ok(BexExternalValue::String(text)) = output.result else {
        panic!("{:?}", output.result);
    };
    for expected in [
        "# config",
        "name = 'app' # unchanged",
        "count = 32 # workers",
        "ratio = 2.5",
        "ready = true",
        "values = [\"a\", 2, false]",
        "day = 2026-10-03",
        "time = 12:30:00.123",
        "local = 2026-10-03T12:30:00",
        "offset = 2026-10-03T12:30:00-07:00",
        "[service] # settings",
        "port = 9000",
    ] {
        assert!(text.contains(expected), "missing {expected:?}: {text}");
    }
}

#[tokio::test]
async fn table_unchanged_document_roundtrips_exactly() {
    let output = baml_test!(
        r##"
        function main() -> bool {
            let source = "# leading\n\"a.b\" = 'literal'\nnums = [\n  0x10, # hex\n  2,\n]\ninline = { count=1, flag = true } # inline\nwhen = 1979-05-27 07:32:00Z\n[[servers]] # first\nport = 8080\n[[servers]]\nport = 9000\n# trailing\n";
            let doc = baml.toml.Table.parse(source);
            doc.to_string() == source && doc.get("missing") == null
        }
    "##
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn table_nested_array_and_inline_edits_keep_comments() {
    let output = baml_test!(
        r##"
        function main() -> string {
            let doc = baml.toml.Table.parse("nums = [\n  1, # first\n  2, # second\n]\ninline = { count=1, flag = true } # inline\n[[servers]] # server\nport = 8080 # port\n");
            match (doc.get("nums")) { let values: baml.toml.Item[] => { values[1] = 3; }, _ => baml.sys.panic("array missing") };
            doc.table("inline").set("flag", false);
            doc.table("inline").table("nested").set("count", 7);
            match (doc.get("servers")) {
                let servers: baml.toml.Item[] => {
                    match (servers[0]) { let server: baml.toml.Table => { server.set("port", 9000); }, _ => baml.sys.panic("server missing") };
                },
                _ => baml.sys.panic("servers missing"),
            };
            let text = doc.to_string();
            let parsed = baml.toml.Table.parse(text);
            if (parsed.table("inline").table("nested").get("count") != 7) {
                baml.sys.panic("nested inline table did not roundtrip");
            }
            text
        }
    "##
    );
    let Ok(BexExternalValue::String(text)) = output.result else {
        panic!("{:?}", output.result);
    };
    for expected in [
        "1, # first",
        "3, # second",
        "flag = false",
        "nested = { count = 7 }",
        "# inline",
        "[[servers]] # server",
        "port = 9000 # port",
    ] {
        assert!(text.contains(expected), "missing {expected:?}: {text}");
    }
}

#[tokio::test]
async fn table_creation_removal_and_rename_are_explicit() {
    let output = baml_test!(
        r##"
        function main() -> bool {
            let doc = baml.toml.Table.parse("[toolchain]\nversion = 'old' # selector\nextra = true\n");
            let toolchain = doc.table("toolchain");
            let missing = toolchain.remove("absent") == null && !toolchain.rename("absent", "other");
            let renamed = toolchain.rename("version", "channel") && toolchain.rename("channel", "path");
            toolchain.set("path", "./cli");
            doc.table("new").table("nested").set("count", 3);
            let text = doc.to_string();
            let parsed = baml.toml.Table.parse(text);
            missing && renamed && toolchain.get("version") == null &&
                text.includes("path = \"./cli\" # selector") &&
                parsed.table("new").table("nested").get("count") == 3 &&
                baml.toml.Table.new().to_string() == ""
        }
    "##
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn table_rejects_invalid_operations_without_changing_values() {
    let output = baml_test!(
        r##"
        function main() -> bool {
            let doc = baml.toml.Table.parse("a = 1\nb = 2\n");
            let collision = { doc.rename("a", "b"); false } catch (e) { baml.errors.InvalidArgument => true };
            let scalar = { doc.table("a"); false } catch (e) { baml.errors.InvalidArgument => true };
            let cycle = baml.toml.Table.new();
            cycle.set("self", cycle);
            let cyclic = { cycle.to_string(); false } catch (e) { baml.panics.UserPanic => true };
            collision && scalar && cyclic && doc.get("a") == 1 && doc.get("b") == 2
        }
    "##
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn table_key_swaps_preserve_each_values_comments() {
    let output = baml_test!(
        r##"
        function main() -> bool {
            let doc = baml.toml.Table.parse("a = 1 # first\nb = 2 # second\n");
            doc.rename("a", "temporary");
            doc.rename("b", "a");
            doc.rename("temporary", "b");
            let text = doc.to_string();
            let source = "\"quoted\" = 'value' # unchanged\nother = 0x10\n";
            let identity = baml.toml.Table.parse(source);
            identity.rename("quoted", "temporary");
            identity.rename("temporary", "quoted");
            text.includes("a = 2 # second") && text.includes("b = 1 # first") &&
                identity.to_string() == source
        }
    "##
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn removed_renamed_key_does_not_donate_comments_to_a_new_key() {
    let output = baml_test!(
        r##"
        function main() -> bool {
            let doc = baml.toml.Table.parse("old = 1 # removed\nkeep = 2 # retained\n");
            doc.rename("old", "new");
            let removed = doc.remove("new") == 1;
            doc.set("new", 9);
            let text = doc.to_string();
            removed && !text.includes("# removed") && text.includes("keep = 2 # retained") &&
                baml.toml.Table.parse(text).get("new") == 9
        }
    "##
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn empty_arrays_of_tables_keep_their_keys() {
    let output = baml_test!(
        r##"
        function main() -> bool {
            let doc = baml.toml.Table.parse("[[servers]]\nport = 8080\n[[cluster.nodes]]\nname = 'primary'\n");
            let empty: baml.toml.Item[] = [];
            doc.set("servers", empty);
            doc.table("cluster").set("nodes", empty);
            let text = doc.to_string();
            let parsed = baml.toml.Table.parse(text);
            let servers = match (parsed.get("servers")) {
                let values: baml.toml.Item[] => values.length() == 0,
                _ => false,
            };
            let nodes = match (parsed.table("cluster").get("nodes")) {
                let values: baml.toml.Item[] => values.length() == 0,
                _ => false,
            };
            if (!servers || !nodes) {
                baml.sys.panic("Empty table arrays did not roundtrip: " + text);
            }
            true
        }
    "##
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn removed_and_reintroduced_keys_do_not_reuse_source_comments() {
    let output = baml_test!(
        r##"
        function main() -> bool {
            let replaced = baml.toml.Table.parse("key = 1 # removed\n");
            replaced.remove("key");
            replaced.set("key", 2);
            let replaced_text = replaced.to_string();

            let reused = baml.toml.Table.parse("a = 1 # original\n");
            reused.rename("a", "b");
            reused.set("a", 2);
            reused.rename("a", "c");
            reused.rename("b", "d");
            let reused_text = reused.to_string();

            let nested = baml.toml.Table.parse("[server] # removed table\nport = 8080 # removed port\n");
            nested.remove("server");
            nested.table("server").set("port", 9090);
            let nested_text = nested.to_string();

            !replaced_text.includes("# removed") &&
                baml.toml.Table.parse(replaced_text).get("key") == 2 &&
                reused_text.includes("d = 1 # original") &&
                baml.toml.Table.parse(reused_text).get("c") == 2 &&
                !nested_text.includes("# removed") &&
                baml.toml.Table.parse(nested_text).table("server").get("port") == 9090
        }
    "##
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
