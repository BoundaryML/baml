// Runtime tests for the `baml.toml` standard-library namespace.
//
// Converted from crates/baml_tests/tests/toml.rs (one test remains there:
// `from_json_skips_null`, which is `#[ignore]`d pending the VM
// `continue`/`break` bug and has no BAML-side ignore mechanism).
//
// Two layers are exercised:
// - The native `baml.toml.Table.parse` (`crates/bex_vm/src/package_baml/toml.rs`),
//   whose `convert_toml_value` maps every `toml::Value` arm onto a VM value,
//   with the four TOML datetime kinds mapped onto the `baml.time` types per
//   BEP-021.
// - The BAML stdlib functions in `ns_toml/toml.baml`: `Table.to_json` /
//   `Table.from_json` and the `item_to_json` / `item_from_json` helpers.
//
// Parse results are inspected by stringifying their JSON projection
// (`baml.json.stringify(table.to_json())`). This is deterministic:
// `toml = "0.8"` is built without `preserve_order`, so `toml::Table` sorts
// keys alphabetically, and `serde_json` carries `preserve_order`, so the
// emitted JSON key order is fixed.

// ─── Native parse path: convert_toml_value arms ──────────────────────────────

// Base case: the string / int / float / bool scalar arms, plus a successful
// `parse` and the `Table.to_json` projection, in one shot.
test "toml_parse_scalars" {
    let t = baml.toml.Table.parse("name = \"hi\"\ncount = 3\nratio = 1.5\nok = true\n");
    assert.equal(
        baml.json.stringify(t.to_json()),
        "{\"count\":3,\"name\":\"hi\",\"ok\":true,\"ratio\":1.5}",
    )
}

// The offset-datetime arm — allocates a `baml.time.ZonedDateTime` — plus its
// `to_json` reached through the union-valued map projection.
test "toml_parse_datetime" {
    let t = baml.toml.Table.parse("dt = 1979-05-27T07:32:00Z");
    assert.equal(baml.json.stringify(t.to_json()), "{\"dt\":\"1979-05-27T07:32:00Z\"}")
}

// All four TOML datetime kinds in one document, mapped per BEP-021's interop
// table: offset datetime → ZonedDateTime, local datetime → PlainDateTime,
// local date → PlainDate, local time → PlainTime.
test "toml_four_datetime_kinds" {
    let t = baml.toml.Table.parse("odt = 1979-05-27T00:32:00-07:00\nldt = 1979-05-27T07:32:00\nld = 1979-05-27\nlt = 07:32:00.5\n");
    assert.equal(
        baml.json.stringify(t.to_json()),
        "{\"ld\":\"1979-05-27\",\"ldt\":\"1979-05-27T07:32:00\",\"lt\":\"07:32:00.5\",\"odt\":\"1979-05-27T00:32:00-07:00\"}",
    )
}

// The array arm (recursion over elements).
test "toml_parse_array" {
    let t = baml.toml.Table.parse("nums = [1, 2, 3]");
    assert.equal(baml.json.stringify(t.to_json()), "{\"nums\":[1,2,3]}")
}

// The nested-table arm (recursive `Table` allocation).
test "toml_parse_nested_table" {
    let t = baml.toml.Table.parse("[server]\nhost = \"localhost\"");
    assert.equal(
        baml.json.stringify(t.to_json()),
        "{\"server\":{\"host\":\"localhost\"}}",
    )
}

// The error branch in `toml.rs`: malformed input throws `TomlParseError`.
test "toml_parse_invalid_throws" {
    {
        baml.toml.Table.parse("= = =");
        baml.sys.panic("expected Table.parse(\"= = =\") to throw")
    } catch (e) {
        baml.toml.TomlParseError => null
    }
}

// ─── BAML-level functions ────────────────────────────────────────────────────

// `item_from_json`'s explicit null-rejection arm — the deliberate contrast
// with `Table.from_json`, which silently skips nulls.
test "toml_item_from_json_rejects_null" {
    {
        baml.toml.item_from_json(null);
        baml.sys.panic("expected item_from_json(null) to throw")
    } catch (e) {
        baml.json.JsonDecodeError => null
    }
}

// `item_to_json` is a public helper that `Table.to_json` does not reach (the
// latter uses the auto-derived map projection), so it is otherwise untested.
// One call hits its array-recursion, `ZonedDateTime`, and scalar arms
// together. Hoisted into a function: locals inside `test` blocks are boxed
// by the closure capture (known VM bug, see the ns_closures/closures.baml
// header), and a boxed local passed to the union-typed `Item` parameter
// fails the match arms.
function toml_item_to_json_array() -> string {
    // Declared `Item[]` (not `Item`) so the literal adopts `Item` as its element
    // type — a genuine array-of-items. Ascribing `Item` instead would infer the
    // narrow element join `ZonedDateTime | int`, which is *not* invariantly an
    // `Item[]`, so the `Item[]` match arm would (correctly) reject it. It is still
    // an `Item` at the top level, so the `item_to_json(item: Item)` call holds.
    let item: baml.toml.Item[] = [baml.time.ZonedDateTime.parse("2020-01-01T00:00:00Z"), 7];
    baml.json.stringify(baml.toml.item_to_json(item))
}
test "toml_item_to_json_array_with_datetime" {
    assert.equal(toml_item_to_json_array(), "[\"2020-01-01T00:00:00Z\",7]")
}
