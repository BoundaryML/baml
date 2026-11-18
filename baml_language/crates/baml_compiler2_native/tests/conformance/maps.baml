// Maps: literals, `m[k]` read and written, every native of `Map`, and what
// growth, deletion and collection must leave alone. Keys and values come
// out of loops so that they are built at run time.

enum MapColor { Red Green Blue }
class MapHolder {
  name: string
  counts: map<string, int>
}

function map_key(i: int) -> string { `k${i}` }
function map_long_key(i: int) -> string { `a-long-key-number-${i}-that-is-sliced` }
function map_squares(n: int) -> map<string, int> {
  let m: map<string, int> = {};
  for (let i = 0; i < n; i += 1) { m[map_key(i)] = i * i; };
  m
}
function map_ints(n: int) -> map<int, int> {
  let m: map<int, int> = {};
  for (let i = 0; i < n; i += 1) { m[i * 7919 - 50] = i; };
  m
}
function map_show(m: map<string, int>) -> string {
  let out = `${m.length()}:`;
  let keys = m.keys();
  let values = m.values();
  for (let i = 0; i < keys.length(); i += 1) { out = `${out}${keys[i]}=${values[i]},`; };
  out
}
function map_show_ints(m: map<int, int>) -> string {
  let out = `${m.length()}:`;
  let keys = m.keys();
  for (let k in keys) { out = `${out}${k}=${m[k]},`; };
  out
}
function map_sum(m: map<string, int>) -> int {
  let total = 0;
  for (let v in m.values()) { total += v; };
  total
}

function map_literal_empty() -> int {
  let m: map<string, int> = {};
  m.length()
}
function map_literal_strings() -> string {
  let m: map<string, int> = { "one": 1, "two": 2, "three": 3 };
  map_show(m)
}
function map_literal_repeated_key() -> string {
  let m: map<string, int> = { "a": 1, "b": 2, "a": 3 };
  map_show(m)
}
function map_literal_large() -> string {
  let m: map<string, int> = { "a": 1, "b": 2, "c": 3, "d": 4, "e": 5, "f": 6, "g": 7, "h": 8, "i": 9, "j": 10, "k": 11 };
  m["l"] = 12;
  map_show(m)
}
function map_literal_ints() -> string {
  let m: map<int, int> = { 3: 30, -1: 10, 0: 0 };
  map_show_ints(m)
}
function map_index_read() -> int {
  let m = map_squares(5);
  m["k3"] + m[map_key(4)]
}
function map_index_write_replaces() -> string {
  let m = map_squares(3);
  m["k1"] = 100;
  m[map_key(0)] = -1;
  map_show(m)
}
function map_index_missing() -> int {
  let m = map_squares(3);
  m[map_key(7)]
}
function map_index_missing_empty() -> int {
  let m: map<int, int> = {};
  m[0]
}
function map_index_missing_caught() -> int {
  let m = map_squares(3);
  let found = m[map_key(9)] catch (e) { _ => -2 };
  found + m["k2"]
}
function map_length() -> int { map_squares(40).length() }
function map_has() -> string {
  let m = map_squares(20);
  `${m.has("k0")},${m.has(map_key(19))},${m.has("k20")},${m.has("")}`
}
function map_get() -> string {
  let m = map_squares(20);
  let a = m.get("k4") ?? -1;
  let b = m.get(map_key(30)) ?? -1;
  `${a},${b}`
}
function map_get_strings() -> string {
  let m: map<string, string> = {};
  for (let i = 0; i < 12; i += 1) { m[map_key(i)] = rep("v", i); };
  let a = m.get("k3") ?? "none";
  let b = m.get("k33") ?? "none";
  `${a},${b}`
}
function map_set_returns_previous() -> string {
  let m = map_squares(3);
  let a = m.set("k1", 50) ?? -1;
  let b = m.set("k9", 60) ?? -1;
  `${a},${b}|${map_show(m)}`
}
function map_delete() -> string {
  let m = map_squares(5);
  let a = m.delete("k1") ?? -1;
  let b = m.delete("k1") ?? -1;
  let c = m.delete(map_key(4)) ?? -1;
  `${a},${b},${c}|${map_show(m)}`
}
function map_get_or_insert() -> string {
  let m = map_squares(2);
  let a = m.get_or_insert("k1", 99);
  let b = m.get_or_insert("k5", 99);
  `${a},${b}|${map_show(m)}`
}
function map_clear() -> string {
  let m = map_squares(30);
  m.clear();
  let empty = map_show(m);
  m["after"] = 1;
  `${empty}|${map_show(m)}|${m.has("k3")}`
}
function map_keys_values() -> string {
  let m = map_squares(4);
  `${show_words(m.keys())}|${show(m.values())}`
}
function map_order_set_delete_set() -> string {
  let m = map_squares(4);
  m["k1"] = 11;
  m.delete("k0");
  m["k0"] = 0;
  m.delete("k2");
  m["k9"] = 9;
  m["k2"] = 2;
  map_show(m)
}
function map_grows_past_small() -> string {
  let m = map_squares(8);
  let before = map_show(m);
  m["k8"] = 64;
  `${before}|${map_show(m)}|${m["k0"]},${m["k8"]}`
}
function map_grows_many() -> string {
  let m = map_squares(3000);
  let ok = 0;
  for (let i = 0; i < 3000; i += 1) { if (m[map_key(i)] == i * i) { ok += 1; } };
  `${m.length()},${ok},${m.has("k3000")},${map_sum(m)}`
}
function map_compacts_after_deletes() -> string {
  let m = map_squares(600);
  for (let i = 0; i < 600; i += 1) { if (i % 10 != 3) { m.delete(map_key(i)); } };
  for (let i = 1000; i < 1400; i += 1) { m[map_key(i)] = i; };
  let keys = m.keys();
  let ok = 0;
  for (let i = 0; i < 600; i += 1) { if (m.has(map_key(i)) == (i % 10 == 3)) { ok += 1; } };
  `${m.length()},${ok},${keys[0]},${keys[59]},${keys[60]},${keys[459]},${m["k1399"]}`
}
function map_delete_all_then_reuse() -> string {
  let m = map_ints(100);
  for (let i = 0; i < 100; i += 1) { m.delete(i * 7919 - 50); };
  let empty = m.length();
  for (let i = 0; i < 20; i += 1) { m[i] = i + 1; };
  `${empty},${m.length()},${m[19]},${m.has(7869)}`
}
function map_slice_keys() -> string {
  let m: map<string, int> = {};
  for (let i = 0; i < 20; i += 1) {
    let whole = map_long_key(i);
    m[whole.slice(2, whole.length() - 3)] = i;
  };
  let probe = map_long_key(13);
  let part = probe.slice(2, probe.length() - 3);
  let flat = `long-key-number-7-that-is-sli`;
  let keys = m.keys();
  `${m[part]},${m[flat]},${m.has(probe)},${keys[19]},${m.length()}`
}
function map_constant_keys() -> string {
  let m: map<string, int> = {};
  m["alpha"] = 1;
  m["a-constant-key-longer-than-two-words"] = 2;
  m[""] = 3;
  m[rep("alph", 1) + "a"] = 4;
  `${m["alpha"]},${m["a-constant-key-longer-than-two-words"]},${m[""]},${m[rep("", 3)]},${m.length()}`
}
function map_non_ascii_keys() -> string {
  let m: map<string, int> = {};
  m["héllo"] = 1;
  m["日本語のキー"] = 2;
  m[rep("é", 9)] = 3;
  m["hello"] = 4;
  `${m["héllo"]},${m["日本語のキー"]},${m[rep("é", 9)]},${m["hello"]},${m.has(rep("é", 8))},${show_words(m.keys())}`
}
function map_int_keys() -> string {
  let m = map_ints(50);
  m[-50] = 1000;
  m.delete(7869);
  `${m[-50]},${m[49 * 7919 - 50]},${m.has(7869)},${m.has(0)},${m.length()}`
}
function map_int_keys_small() -> string {
  let m = map_ints(5);
  m.delete(-50);
  m[1] = 1;
  map_show_ints(m)
}
function map_bool_keys() -> string {
  let m: map<bool, string> = {};
  m[true] = "yes";
  m[1 > 2] = "no";
  m[true] = "aye";
  `${m[false]},${m[true]},${m.length()}`
}
function map_enum_keys() -> string {
  let m: map<MapColor, int> = {};
  m[MapColor.Blue] = 3;
  m[MapColor.Red] = 1;
  m[MapColor.Blue] = 33;
  let green = m.get(MapColor.Green) ?? -1;
  `${m[MapColor.Blue]},${m[MapColor.Red]},${green},${m.length()}`
}
function map_float_keys() -> string {
  let m: map<float, int> = {};
  m[halves(3)] = 15;
  m[0.0] = 1;
  m[1.5] = 16;
  let missing = m.get(2.5) ?? -1;
  `${m[1.5]},${m[halves(0)]},${missing},${m.length()}`
}
function map_float_values() -> float {
  let m: map<string, float> = {};
  for (let i = 0; i < 12; i += 1) { m[map_key(i)] = halves(i); };
  m["k11"] + (m.get("k3") ?? 100.0)
}
function map_bool_values() -> string {
  let m: map<int, bool> = {};
  for (let i = 0; i < 12; i += 1) { m[i] = i % 3 == 0; };
  let none = m.get(40) ?? true;
  `${m[3]},${m[4]},${m.get(9) ?? false},${none}`
}
function map_object_values() -> string {
  let m: map<string, Named> = {};
  for (let p in people(12)) { m[p.name] = p; };
  let found = m.get("pppp");
  let rank = if (found == null) { -1 } else { found.rank };
  `${m["ppppppppppp"].rank},${rank},${ranks(m.values())}`
}
function map_class_field() -> string {
  let holder = MapHolder { name: "h", counts: {} };
  for (let i = 0; i < 10; i += 1) { holder.counts[map_key(i % 4)] = i; };
  holder.counts["k1"] += 100;
  map_show(holder.counts)
}
function map_array_elements() -> string {
  let maps: map<string, int>[] = [];
  for (let i = 0; i < 4; i += 1) { maps.push(map_squares(i)); };
  maps[2]["extra"] = 5;
  `${map_show(maps[0])}|${map_show(maps[2])}|${maps[3]["k2"]}`
}
function map_of_maps() -> string {
  let outer: map<string, map<int, int>> = {};
  for (let i = 0; i < 10; i += 1) { outer[map_key(i)] = map_ints(i); };
  outer["k3"][5] = 55;
  `${outer["k9"][8 * 7919 - 50]},${outer["k3"][5]},${outer["k0"].length()},${outer.length()}`
}
function map_of_arrays() -> string {
  let m: map<string, int[]> = {};
  for (let i = 0; i < 10; i += 1) { m[map_key(i)] = ints(i); };
  m["k2"].push(7);
  `${show(m["k2"])}|${show(m["k0"])}|${m["k9"].length()}`
}
function map_shared_reference() -> string {
  let m = map_squares(2);
  let same = m;
  same["k5"] = 25;
  m.delete("k0");
  map_show(same)
}
function map_counts_words() -> string {
  let counts: map<string, int> = {};
  for (let i = 0; i < 500; i += 1) {
    let word = map_key(i % 17);
    counts[word] = (counts.get(word) ?? 0) + 1;
  };
  `${counts["k0"]},${counts["k16"]},${counts.length()}`
}
function map_equal_any_order() -> string {
  let a = map_squares(12);
  let b: map<string, int> = {};
  for (let i = 11; i >= 0; i -= 1) { b[map_key(i)] = i * i; };
  let same = a == b;
  b["k3"] = 10;
  let changed = a == b;
  b["k3"] = 9;
  b["k12"] = 144;
  let longer = a != b;
  b.delete("k12");
  b.delete("k0");
  b["other"] = 0;
  `${same},${changed},${longer},${a == b},${a == a}`
}
function map_equal_strings() -> string {
  let a: map<string, string> = {};
  let b: map<string, string> = {};
  for (let i = 0; i < 5; i += 1) { a[map_key(i)] = rep("v", i); b[map_key(4 - i)] = rep("v", 4 - i); };
  let same = a == b;
  b["k2"] = "vvv";
  let empty: map<string, string> = {};
  let other: map<string, string> = {};
  `${same},${a == b},${a != b},${empty == other},${empty == a}`
}

// A missing key is a `baml.panics.MapKeyNotFound`, a class like any other:
// caught by its name and by `Panic`, passed on by a handler that takes
// every error, and reported with the key the interpreter gives it.
function map_missing_dive(depth: int, m: map<string, int>, key: string) -> int {
  if depth > 0 { return map_missing_dive(depth - 1, m, key) + 1; }
  m[key]
}
function map_missing_by_class() -> string {
  let m = map_squares(3);
  let local = m[map_key(9)] catch (e) {
    baml.panics.MapKeyNotFound => {
      let key = e.key;
      key.length()
    },
  };
  let deep = map_missing_dive(3, m, map_key(9)) catch (e) {
    baml.panics.MapKeyNotFound => {
      let key = e.key;
      key.length() * 10
    },
  };
  let ints = map_ints(3);
  let number = ints[1] catch (e) {
    baml.panics.MapKeyNotFound => {
      let key = e.key;
      key.length() * 100
    },
  };
  `${local},${deep},${number},${m["k2"]}`
}
function map_missing_key_text() -> string {
  let m: map<int, string> = {};
  m[3] = rep("v", 3);
  m[2] catch (e) { baml.panics.MapKeyNotFound => e.key }
}
function map_missing_by_alias() -> int {
  let m = map_ints(3);
  let local = m[1] catch (e) { baml.panics.Panic => -3 };
  let deep = map_missing_dive(2, map_squares(2), map_key(5)) catch (e) { baml.panics.Panic => -4 };
  local * 10 + deep
}
function map_missing_passes_inner(m: map<string, int>) -> int {
  map_missing_dive(2, m, map_key(5)) catch (e) { _ => -1 }
}
function map_missing_passes_catch_all() -> int {
  let m = map_squares(3);
  map_missing_passes_inner(m) catch (e) { baml.panics.MapKeyNotFound => m["k2"] + 100 }
}
function map_missing_other_arms() -> int {
  let m = map_squares(3);
  map_missing_dive(1, m, map_key(8)) catch (e) {
    baml.panics.IndexOutOfBounds => -1,
    baml.panics.DivisionByZero => -2,
    baml.panics.MapKeyNotFound => 5,
  }
}
function map_missing_uncaught_through_frames() -> int { map_missing_dive(3, map_squares(2), map_key(4)) }
function map_missing_uncaught_past_catch_all() -> int { map_missing_passes_inner(map_squares(1)) }

// Maps a frame holds while an error goes through it are the ones its
// handler reads and writes, wherever a collection has moved them.
function map_held_across_throw() -> string {
  let m: map<string, string> = {};
  for (let i = 0; i < 12; i += 1) { m[map_long_key(i)] = rep("v", i); };
  let counts = map_squares(10);
  let r = map_missing_dive(4, counts, map_key(40)) catch (e) {
    baml.panics.MapKeyNotFound => {
      m[e.key] = rep("x", 3);
      counts[map_key(40)] = m.length();
      counts[map_key(40)] + counts["k3"]
    },
  };
  `${r}|${m.get("(unknown)") ?? "none"}|${m.get(map_long_key(11)) ?? "none"}|${counts.length()}`
}

// A panic part of the way through filling a map leaves what was set.
function map_fill(m: map<string, int>, n: int) -> int {
  for (let i = 0; i < n; i += 1) { m[map_key(i)] = 100 / (5 - i); };
  n
}
function map_panic_while_building() -> string {
  let m: map<string, int> = {};
  let r = map_fill(m, 12) catch (e) { baml.panics.DivisionByZero => m.length() * 100 };
  m["after"] = r;
  map_show(m)
}
function map_literal_value_panics() -> string {
  let source = map_squares(2);
  let m: map<string, int> = { "a": source["k1"], "b": source[map_key(7)], "c": 3 } catch (e) {
    baml.panics.MapKeyNotFound => map_squares(source.length()),
  };
  map_show(m)
}

// An uncaught instance of a class that holds a map: the class's
// description says what the map's keys and values are.
function map_uncaught_holder() -> int throws MapHolder {
  let counts = map_squares(2);
  if counts.length() == 2 { throw MapHolder { name: "h", counts: counts } }
  0
}
