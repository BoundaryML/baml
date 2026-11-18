// A part of a string long enough not to be copied is a slice of it: these
// are parts that long, read every way a string is read, and held every way
// the collector finds a string while the string they are parts of is and is
// not otherwise alive.

function slice_text(n: int) -> string { rep("0123456789abcdefghijklmnopqrstuvwxyz", n) }
function slice_wide(n: int) -> string { rep("é1😀→ü", n) }
function slice_kept(n: int) -> string {
  let whole = slice_text(n);
  whole.slice(3, 43)
}
function slice_kept_wide(n: int) -> string {
  let whole = slice_wide(n);
  whole.slice(2, 42)
}

function slice_long_middle() -> string { slice_text(4).slice(5, 60) }
function slice_long_from_end() -> string { slice_text(4).slice(-50, -3) }
function slice_long_prefix() -> string { slice_text(4).slice(0, 40) }
function slice_long_suffix() -> string { slice_text(4).slice(100, 144) }
function slice_long_whole() -> string {
  let s = slice_text(2);
  let t = s.slice(0, 72);
  `${t}|${t == s}`
}
function slice_long_clamps() -> string { slice_text(2).slice(-1000, 1000) }
function slice_long_out_of_range() -> string {
  let s = slice_text(2);
  `[${s.slice(72, 200)}|${s.slice(100, 200)}|${s.slice(60, 10)}|${s.slice(-200, -100)}|${s.slice(-40, 200)}]`
}
function slice_around_shortest() -> string {
  let s = slice_text(3);
  `${s.slice(1, 32)}|${s.slice(1, 33)}|${s.slice(1, 34)}|${s.slice(1, 32).length()}`
}
function slice_empty_ones() -> string {
  let s = slice_text(4).slice(2, 100);
  `[${s.slice(40, 40)}|${s.slice(98, 98)}|${s.slice(0, 0)}|${s.slice(40, 40).length()}|${s.slice(40, 40) == ""}]`
}
function slice_of_slice() -> string { slice_text(4).slice(10, 130).slice(5, 100).slice(-60, -2) }
function slice_of_slice_short() -> string { slice_text(4).slice(10, 130).slice(7, 12) }
function slice_of_slice_whole() -> string {
  let s = slice_text(4).slice(10, 130);
  `${s.slice(0, 120) == s}|${s.slice(0, 120).length()}`
}
function slice_of_constant() -> string {
  let s = if (rep("a", 1) == "a") { "the quick brown fox jumps over the lazy dog, twice over" } else { "other" };
  let junk = rep("j", 3);
  `${s.slice(4, 50)}|${s.slice(4, 50).slice(6, 40)}|${junk}`
}

function slice_wide_middle() -> string { slice_wide(20).slice(3, 70) }
function slice_wide_lengths() -> string {
  let s = slice_wide(20).slice(7, 81);
  `${s.length()}|${s.byte_length()}|${s.slice(1, 60).length()}|${s.at(0) ?? "none"}|${s.at(-1) ?? "none"}|${s.code_point_at(3) ?? -1}`
}
function slice_wide_of_slice() -> string { slice_wide(20).slice(2, 90).slice(3, 50).slice(-30, -1) }
function slice_wide_from_end() -> string { slice_wide(20).slice(-41, -2) }
function slice_ascii_part_of_wide() -> string {
  let s = `${rep("é", 3)}${slice_text(2)}${rep("ü", 2)}`.slice(3, 70);
  `${s.is_ascii()}|${s.slice(2, 40)}|${s.length()}|${s.index_of("z") ?? -1}`
}
function slice_wide_part_of_wide() -> string {
  let s = slice_wide(20).slice(1, 60);
  `${s.is_ascii()}|${s.index_of("→") ?? -1}|${s.last_index_of("é") ?? -1}|${s.slice(4, 44)}`
}

function slice_equality() -> string {
  let a = slice_text(4).slice(36, 108);
  let b = slice_text(2);
  `${a == b}|${a != b}|${a == a}|${a.slice(0, 40) == b.slice(0, 40)}|${a.slice(0, 40) == b.slice(1, 41)}|${a.slice(0, 40) == a.slice(36, 76)}`
}
function slice_ordering() -> string {
  let a = slice_text(4).slice(36, 108);
  let b = slice_text(2);
  `${a < b}|${a <= b}|${a.slice(1, 50) > b}|${a.slice(0, 50) < b}|${a.slice(0, 50) >= b.slice(0, 50)}|${a.slice(2, 50) < a.slice(1, 50)}`
}
function slice_concatenation() -> string {
  let s = slice_text(4);
  s.slice(0, 40) + s.slice(100, 140) + `${s.slice(50, 90)}!` + s.slice(3, 40).slice(1, 36)
}
function slice_searches() -> string {
  let s = slice_text(4).slice(3, 120);
  `${s.length()}|${s.index_of("xyz") ?? -1}|${s.last_index_of("012") ?? -1}|${s.includes("z0")}|${s.includes("z1")}|${s.starts_with("345")}|${s.starts_with("012")}|${s.ends_with("def")}|${s.ends_with("efg")}`
}
function slice_characters() -> string {
  let s = slice_text(4).slice(3, 120);
  `${s.code_point_at(5) ?? -1}|${s.at(-1) ?? "none"}|${s.at(0) ?? "none"}|${s.chars().length()}|${s.to_code_points()[116]}|${s.byte_length()}|${s.is_ascii_alphanumeric()}|${s.is_ascii_numeric()}`
}
function slice_rebuilt() -> string {
  let s = slice_text(4).slice(3, 43);
  `${s.to_upper_case()}|${s.to_upper_case().to_lower_case() == s}|${s.repeat(2)}|${s.replace("9a", "--")}|${s.replace_all("b", "")}`
}
function slice_to_string() -> string {
  let s = slice_text(4).slice(3, 43);
  let t = s.to_string();
  `${t}|${t == s}|${s}`
}
function slice_joined() -> string {
  let s = slice_text(4);
  let xs = [s.slice(0, 36), s.slice(36, 40), s.slice(40, 100), s];
  `${xs.join("-")}|${xs.join("").length()}`
}

function slice_split_long_parts() -> string {
  let p = rep(`${slice_text(1)},`, 5).split(",");
  `${p.length()}:${p[0]}|${p[4]}|${p[5]}|${p[2] == p[3]}|${p[4].length()}`
}
function slice_split_mixed_parts() -> string {
  let line = `${rep("a", 31)};${rep("b", 32)};${rep("c", 33)};;x;${rep("d", 40)}`;
  let p = line.split(";");
  `${p.length()}:${p[0].length()}${p[1].length()}${p[2].length()}${p[3].length()}${p[4]}${p[5]}|${p[1]}|${p[2].slice(1, 33)}`
}
function slice_split_long_delimiter() -> string {
  let p = rep(`${slice_text(1)}<=>`, 3).split("<=>");
  `${p.length()}:${p[1]}|${p[2].slice(-34, -1)}|${p[3]}|`
}
function slice_split_wide() -> string {
  let p = rep(`${slice_wide(8)},`, 3).split(",");
  `${p.length()}:${p[1]}|${p[2].length()}|${p[2].byte_length()}|${p[0].slice(3, 37)}`
}
function slice_split_of_slice() -> string {
  let s = rep(`${slice_text(1)},`, 6).slice(20, 200);
  let p = s.split(",");
  `${p.length()}:${p[0]}|${p[1]}|${p[4]}|${p[5]}`
}
function slice_split_no_match() -> string {
  let s = slice_text(2).slice(1, 70);
  let p = s.split("!");
  `${p.length()}:${p[0] == s}`
}
function slice_lines_long() -> string {
  let l = rep(`${slice_text(1)}\n${slice_text(2)}\r\nshort\n\n`, 2).lines();
  `${l.length()}:${l[0]}|${l[1].length()}|${l[2]}|${l[3]}|${l[5]}|${l[4] == l[0]}`
}
function slice_lines_wide() -> string {
  let l = rep(`${slice_wide(8)}\n`, 3).lines();
  `${l.length()}:${l[2]}|${l[1].length()}`
}
function slice_trim_long() -> string {
  let s = `${rep(" ", 5)}${slice_text(2)}${rep("\t", 3)}`;
  `[${s.trim()}|${s.trim_start().length()}|${s.trim_end().length()}|${s.trim().trim() == s.trim()}|${s.trim_start().trim_end()}]`
}
function slice_trim_wide() -> string {
  let wide = rep("　", 2);
  let s = ` ${wide}${slice_wide(10)} x${wide} `;
  `[${s.trim()}|${s.trim().length()}|${s.trim_start().length()}|${s.trim_end().length()}|${s.trim().byte_length()}]`
}
function slice_trim_of_slice() -> string {
  let s = `${slice_text(1)}${rep(" ", 6)}${slice_text(2)}${rep(" ", 6)}${slice_text(1)}`.slice(36, 120);
  `[${s.trim()}|${s.trim_start().length()}|${s.trim_end().length()}]`
}

// The collector.

function slice_outlives_its_string() -> string {
  let part = slice_kept(4);
  let junk = rep("x", 50);
  `${part}|${junk.length()}|${part.length()}`
}
function slice_outlives_its_wide_string() -> string {
  let part = slice_kept_wide(20);
  let junk = rep("x", 50);
  `${part}|${junk.length()}|${part.length()}|${part.byte_length()}`
}
function slice_of_a_slice_that_became_a_string() -> string {
  let part = slice_kept(4);
  let junk = rep("x", 5);
  let inner = part.slice(2, 38);
  let more = rep("y", 5);
  `${inner}|${part.slice(1, 5)}|${junk}${more}`
}
function slice_before_its_string() -> string {
  let s = slice_text(4);
  let xs: string[] = [s.slice(2, 50), s.slice(60, 120), s];
  let junk = rep("y", 10);
  `${xs[0]}|${xs[1]}|${xs[2].length()}|${junk}`
}
function slice_after_its_string() -> string {
  let s = slice_text(4);
  let xs: string[] = [s, s.slice(2, 50), s.slice(60, 120)];
  let junk = rep("y", 10);
  `${xs[1]}|${xs[2]}|${xs[0].length()}|${junk}`
}
function slice_many_without_their_string() -> string {
  let parts = rep(`${slice_text(1)},`, 12).split(",");
  let junk = rep("q", 20);
  let out = "";
  for (let p in parts) { out = out + p.slice(0, 3) + p.length().to_string(); };
  `${out}|${parts[7]}|${junk.length()}`
}
function slice_many_with_their_string() -> string {
  let s = rep(`${slice_text(1)},`, 12);
  let parts = s.split(",");
  let junk = rep("q", 20);
  let out = "";
  for (let p in parts) { out = out + p.slice(0, 3) + p.length().to_string(); };
  `${out}|${parts[7]}|${junk.length()}|${s.length()}`
}
function slice_overlapping(n: int) -> string[] {
  let s = slice_text(4);
  let xs: string[] = [];
  for (let i = 0; i < n; i += 1) { xs.push(s.slice(i, 140)); };
  xs
}
function slice_many_as_large_as_their_string() -> string {
  let xs = slice_overlapping(6);
  let junk = rep("w", 9);
  `${xs[0].length()}|${xs[5]}|${xs[3].slice(0, 4)}|${junk}`
}
function slice_few_smaller_than_their_string() -> string {
  let xs = slice_overlapping(1);
  let junk = rep("w", 9);
  `${xs[0]}|${junk}`
}
class SliceHolder {
  text: string
  again: string?
}
function slice_in_a_field() -> string {
  let h = SliceHolder { text: slice_kept(3), again: null };
  let junk = rep("z", 12);
  h.again = h.text;
  let more = rep("z", 3);
  `${h.text}|${h.again ?? "none"}|${junk}${more}`
}
function slice_in_a_closure() -> string {
  let part = slice_kept(4);
  let f = () -> string { part + "!" };
  let junk = rep("c", 7);
  `${f()}|${junk}`
}
function slice_in_many_places() -> string {
  let part = slice_kept(4);
  let h = SliceHolder { text: part, again: part };
  let xs = [part, h.text, part];
  let junk = rep("m", 7);
  `${xs[0] == xs[2]}|${h.again ?? ""}|${xs[1]}|${junk}`
}
function slice_through_calls(part: string, depth: int) -> string {
  if (depth > 0) { return slice_through_calls(part, depth - 1); }
  let junk = rep("r", 4);
  part + junk
}
function slice_on_the_stack() -> string { slice_through_calls(slice_kept(5), 3) }
function slice_built_in_a_loop() -> string {
  let s = slice_text(6);
  let out: string[] = [];
  for (let i = 0; i < 20; i += 1) {
    let part = s.slice(i, i + 60);
    if (i % 5 == 0) { out.push(part); };
  };
  `${out.length()}|${out[3]}|${out[0].slice(1, 40)}`
}

// Thrown, caught, and live across a throw.

function slice_thrown_from(depth: int, part: string) -> int throws string {
  if depth > 0 { return slice_thrown_from(depth - 1, part) + 1; }
  let junk = rep("t", 3);
  if junk.length() == 3 { throw part.slice(1, 39) }
  0
}
function slice_caught_as_a_string() -> string {
  let part = slice_kept(4);
  let found = "";
  let depth = slice_thrown_from(4, part) catch (e) {
    string => { found = e + rep("!", 2); -1 },
  };
  `${found}|${depth}|${part}`
}
function slice_live_into_a_handler() -> string {
  let part = slice_kept(4);
  let depth = slice_thrown_from(4, rep("u", 50)) catch (e) {
    _ => { let junk = rep("h", 6); part.length() + junk.length() },
  };
  `${part}|${depth}`
}
function slice_thrown_and_not_caught() -> int throws string { slice_thrown_from(3, slice_text(2)) }
