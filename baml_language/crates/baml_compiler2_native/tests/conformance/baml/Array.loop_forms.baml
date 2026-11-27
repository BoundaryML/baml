function array_map_ints() -> string { show(ints(4).map((x) -> { x + 1 })) }
function array_map_to_strings() -> string { show_words(ints(3).map((x) -> { rep("m", x / 10 + 1) })) }
function array_map_objects() -> string { show(people(3).map((p) -> { p.rank * 2 })) }
function array_filter_ints() -> string { show(ints(6).filter((x) -> { x % 20 == 0 })) }
function array_filter_objects() -> string { ranks(people(5).filter((p) -> { p.rank > 2 })) }
function array_some_every() -> string {
  let xs = ints(4);
  `${xs.some((x) -> { x == 20 })}${xs.some((x) -> { x == 21 })}${xs.every((x) -> { x >= 0 })}${xs.every((x) -> { x > 0 })}`
}
function array_find() -> string {
  let found = people(4).find((p) -> { p.rank == 2 });
  let missing = ints(3).find((x) -> { x > 99 }) ?? -1;
  let name = if (found == null) { "none" } else { found.name };
  `${name}|${missing}|${ints(5).find_index((x) -> { x == 30 }) ?? -1}`
}
function array_reduce() -> int { ints(5).reduce((sum, x) -> { sum + x }, count(1)) }
function pick_step(n: int) -> (int) -> int throws never {
  if (count(n) > 1) { (x) -> { x * 3 } } else { (x) -> { x + 1 } }
}
function array_map_with_a_function_value() -> string {
  let f = pick_step(2);
  let g = pick_step(1);
  `${show(ints(3).map(f))}|${show(ints(3).map(g))}|${show(ints(4).filter((x) -> { f(x) > 30 }))}`
}

// A pseudo-random run of ints, the same every time.
function scrambled(n: int) -> int[] {
  let out: int[] = [];
  let seed = 7;
  for (let i = 0; i < n; i += 1) {
    seed = seed * 16807 % 2147483647;
    out.push(seed % 50);
  };
  out
}
// People whose ranks repeat, each named for where it started.
function crowd(n: int) -> Named[] {
  let out: Named[] = [];
  let ranks = scrambled(n);
  for (let i = 0; i < n; i += 1) { out.push(Named { name: `n${i}`, rank: ranks[i] % 5 }); };
  out
}
function by_rank(a: Named, b: Named) -> baml.ops.Ordering throws never { a.rank.cmp(b.rank) }
// Equal elements keep the order they came in.
function array_sort_by_is_stable() -> string {
  let xs = crowd(23);
  let sorted = xs.sort_by((a, b) -> { a.rank.cmp(b.rank) });
  `${ranks(sorted)}|${ranks(xs) == ranks(sorted)}`
}
function array_sort_by_descending() -> string {
  show(scrambled(40).sort_by((a, b) -> { b.cmp(a) }))
}
function array_sort_by_sizes() -> string {
  let out = "";
  for (let n = 0; n < 10; n += 1) {
    out = out + show(scrambled(n).sort_by((a, b) -> { a.cmp(b) })) + "|";
  }
  out
}
function array_sort_by_a_function_value() -> string {
  let compare = by_rank;
  ranks(crowd(17).sort_by(compare))
}
// A comparator that throws leaves the array as it was.
function array_sort_by_throws() -> string {
  let xs = scrambled(12);
  let calls = 0;
  let thrown = xs.sort_by((a, b) -> {
    calls += 1;
    if (calls == 9) { throw "ninth" }
    a.cmp(b)
  }) catch (e) { string => [e.length()] };
  `${show(thrown)}|${show(xs)}`
}
// What a comparator does to the array is discarded: the sorted elements are
// the ones it had when the sort began.
function array_sort_by_changes_the_array() -> string {
  let xs = scrambled(9);
  let grown = scrambled(6);
  let shrunk = scrambled(6);
  xs.sort_by((a, b) -> { xs[0] = 99; a.cmp(b) });
  grown.sort_by((a, b) -> { grown.push(77); a.cmp(b) });
  shrunk.sort_by((a, b) -> { if (shrunk.length() > 2) { shrunk.pop(); }; a.cmp(b) });
  `${show(xs)}|${show(grown)}|${show(shrunk)}`
}
function array_find_last() -> string {
  let xs = scrambled(30);
  let found = xs.find_last((x) -> { x % 7 == 3 }) ?? -1;
  let missing = xs.find_last((x) -> { x > 99 }) ?? -1;
  let person = crowd(12).find_last((p) -> { p.rank == 2 });
  let name = if (person == null) { "none" } else { person.name };
  let empty = ints(0).find_last((x) -> { x == 0 }) ?? -2;
  `${found}|${missing}|${name}|${empty}`
}
function array_find_last_index() -> string {
  let xs = scrambled(30);
  let seen: int[] = [];
  let at = xs.find_last_index((x) -> { seen.push(x); x % 7 == 3 }) ?? -1;
  `${at}|${xs.find_last_index((x) -> { x > 99 }) ?? -1}|${ints(0).find_last_index((x) -> { x == 0 }) ?? -2}|${show(seen)}`
}
function array_flat_map() -> string {
  let xs = ints(4);
  let spread = xs.flat_map((x) -> { [x, x + 1] });
  let none = xs.flat_map((x) -> { ints(0) });
  let uneven = xs.flat_map((x) -> { ints(x / 10) });
  let words = people(3).flat_map((p) -> { [p.name, rep("w", p.rank)] });
  `${show(spread)}|${show(none)}|${show(uneven)}|${show_words(words)}|${show(ints(0).flat_map((x) -> { [x] }))}`
}
function array_flat_map_throws() -> string {
  let xs = ints(5);
  let out = xs.flat_map((x) -> {
    if (x == 30) { throw "thirty" }
    [x]
  }) catch (e) { string => [e.length()] };
  `${show(out)}|${show(xs)}`
}

function array_sort_ints() -> string {
  let xs = scrambled(40);
  xs.push(0 - 7);
  let same = xs.sort();
  same.push(1000);
  `${show(xs)}|${show(ints(0).sort())}|${show(ints(1).sort())}`
}
function array_sort_strings() -> string {
  let xs: string[] = [];
  for (let x in scrambled(14)) { xs.push(rep("b", x % 4) + rep("a", x % 3) + `${x % 2}`); }
  xs.push("é");
  xs.push("B");
  xs.push("");
  show_words(xs.sort())
}
function array_sort_floats() -> string {
  let xs: float[] = [];
  for (let x in scrambled(12)) { xs.push(x / 4.0 - 5.0); }
  xs.push(halves(0) / halves(0));
  xs.push(0.0 - halves(0));
  xs.push(halves(0));
  xs.push(tens(400));
  xs.push(0.0 - tens(400));
  xs.sort();
  let out = "";
  for (let x in xs) { out = `${out}${x},`; }
  out
}
function array_sort_bigints() -> string {
  let xs: bigint[] = [];
  let p = 1n;
  for (let x in scrambled(10)) {
    p = p * 1000003n;
    xs.push(if (x % 2 == 0) { p - 0n } else { 0n - p + x });
  }
  xs.sort();
  let out = "";
  for (let x in xs) { out = `${out}${x},`; }
  out
}
