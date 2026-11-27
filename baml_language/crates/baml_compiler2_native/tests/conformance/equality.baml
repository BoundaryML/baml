// `==` on what no one instruction compares: instances field by field,
// arrays and maps of them, and the `eq` of a class that has one, called
// for the instances met inside other values.

class EqPoint {
  x int
  y int
}
enum EqColor {
  Red
  Green
}
class EqNamed {
  name string
  tags string[]
  score float
  inner EqPoint?
  color EqColor
  big bigint
  raw uint8array
}
// Equal whatever the case of its text.
class EqCase {
  text string
  implements baml.ops.Equals {
    function eq(self, other: Self) -> bool throws never {
      self.text.to_lower_case() == other.text.to_lower_case()
    }
  }
}
class EqHolder {
  item EqCase
  count int
}
class EqNode {
  value int
  next EqNode?
}

function eq_point(x: int, y: int) -> EqPoint { EqPoint { x: x, y: y } }
function eq_named(n: int, score: float) -> EqNamed {
  EqNamed {
    name: rep("n", n),
    tags: [rep("a", n), rep("b", 2)],
    score: score,
    inner: if (n > 1) { eq_point(n, n) } else { null },
    color: if (n % 2 == 0) { EqColor.Red } else { EqColor.Green },
    big: big_two_to(70 + n),
    raw: u8_counted(n, 3),
  }
}
function eq_case(text: string, n: int) -> EqCase { EqCase { text: rep(text, n) } }

function eq_points() -> string {
  `${eq_point(1, 2) == eq_point(1, 2)} ${eq_point(1, 2) == eq_point(1, 3)} ${eq_point(1, 2) != eq_point(1, 2)} ${eq_point(1, 2) != eq_point(2, 2)}`
}
function eq_same_instance() -> bool {
  let point = eq_point(4, 5);
  point == point
}
function eq_nested() -> string {
  `${eq_named(3, halves(3)) == eq_named(3, halves(3))} ${eq_named(3, halves(3)) == eq_named(3, halves(4))} ${eq_named(1, halves(1)) == eq_named(1, halves(1))} ${eq_named(2, halves(1)) == eq_named(4, halves(1))}`
}
// A NaN equals itself, and the two zeros are equal.
function eq_float_fields() -> string {
  `${eq_named(2, float.nan()) == eq_named(2, float.nan())} ${eq_named(2, halves(0) * (0.0 - 1.0)) == eq_named(2, halves(0))} ${eq_named(2, float.nan()) == eq_named(2, float.inf())}`
}
function eq_optional() -> string {
  let none: EqPoint? = if (rep("a", 1) == "b") { eq_point(0, 0) } else { null };
  let some: EqPoint? = eq_point(1, 1);
  `${none == none} ${none == some} ${some == eq_point(1, 1)} ${some == none}`
}
function eq_arrays() -> string {
  let one = [eq_point(1, 2), eq_point(3, 4)];
  `${one == [eq_point(1, 2), eq_point(3, 4)]} ${one == [eq_point(1, 2)]} ${one == [eq_point(1, 2), eq_point(3, 5)]} ${one != [eq_point(1, 2), eq_point(3, 4)]}`
}
function eq_float_arrays() -> string {
  let one: float[] = [float.nan(), halves(0), halves(3)];
  let other: float[] = [float.nan(), halves(0) * (0.0 - 1.0), halves(3)];
  `${one == other} ${one == [float.nan(), halves(0)]} ${[halves(1)] == [halves(2)]}`
}
function eq_nested_arrays() -> string {
  let one = [[sort_scrambled(3), sort_scrambled(2)], [sort_scrambled(0)]];
  `${one == [[sort_scrambled(3), sort_scrambled(2)], [sort_scrambled(0)]]} ${one == [[sort_scrambled(3), sort_scrambled(3)], [sort_scrambled(0)]]}`
}
function eq_maps() -> string {
  let one: map<string, EqPoint> = { rep("a", 1): eq_point(1, 1), rep("b", 1): eq_point(2, 2) };
  let same: map<string, EqPoint> = { rep("b", 1): eq_point(2, 2), rep("a", 1): eq_point(1, 1) };
  let other: map<string, EqPoint> = { rep("a", 1): eq_point(1, 1), rep("c", 1): eq_point(2, 2) };
  let changed: map<string, EqPoint> = { rep("a", 1): eq_point(1, 1), rep("b", 1): eq_point(2, 3) };
  `${one == same} ${one == other} ${one == changed}`
}
function eq_maps_of_int_keys() -> string {
  let one: map<int, string[]> = { 1: [rep("a", 2)], 2: [] };
  let same: map<int, string[]> = { 2: [], 1: [rep("a", 2)] };
  let other: map<int, string[]> = { 1: [rep("a", 3)], 2: [] };
  `${one == same} ${one == other}`
}
function eq_unions() -> string {
  let one: int | string | EqPoint = eq_point(1, 2);
  let same: int | string | EqPoint = eq_point(1, 2);
  let text: int | string | EqPoint = rep("1", 1);
  let number: int | string | EqPoint = rep("1", 1).length();
  `${one == same} ${one == text} ${text == number} ${number == number}`
}
// The `eq` of the class is what `==` on two of its instances is.
function eq_own() -> string {
  `${eq_case("ABC", 1) == eq_case("abc", 1)} ${eq_case("ABC", 1) == eq_case("abd", 1)} ${eq_case("ABC", 1) != eq_case("abc", 1)}`
}
// The same `eq`, for instances met inside other values.
function eq_own_inside() -> string {
  let one = EqHolder { item: eq_case("ABC", 2), count: 1 };
  `${one == EqHolder { item: eq_case("abc", 2), count: 1 }} ${one == EqHolder { item: eq_case("abd", 2), count: 1 }} ${one == EqHolder { item: eq_case("abc", 2), count: 2 }}`
}
function eq_own_in_arrays() -> string {
  let one = [eq_case("A", 1), eq_case("B", 2), eq_case("C", 3)];
  `${one == [eq_case("a", 1), eq_case("b", 2), eq_case("c", 3)]} ${one == [eq_case("a", 1), eq_case("x", 2), eq_case("c", 3)]} ${one == [eq_case("a", 1), eq_case("b", 2)]}`
}
function eq_own_in_maps() -> bool {
  let one: map<string, EqCase[]> = { rep("k", 1): [eq_case("Long Text ", 8)], rep("j", 1): [] };
  let same: map<string, EqCase[]> = { rep("j", 1): [], rep("k", 1): [eq_case("long text ", 8)] };
  one == same
}
// Many pairs are left to `eq`, each called once nothing else differs.
function eq_own_many() -> string {
  let one: EqHolder[] = [];
  let other: EqHolder[] = [];
  for (let i = 0; i < 200; i += 1) {
    one.push(EqHolder { item: eq_case("Ab", i % 7 + 1), count: i });
    other.push(EqHolder { item: eq_case("aB", i % 7 + 1), count: i });
  };
  let same = one == other;
  other[199].item = eq_case("zz", 1);
  `${same} ${one == other}`
}
// A value that holds itself is compared without end otherwise.
function eq_cyclic() -> string {
  let one = EqNode { value: 1, next: null };
  one.next = one;
  let same = EqNode { value: 1, next: null };
  same.next = same;
  let other = EqNode { value: 2, next: null };
  other.next = other;
  `${one == same} ${one == other}`
}
function eq_default_called() -> string {
  `${baml._equals_structural_default(eq_point(1, 2), eq_point(1, 2))} ${baml._equals_structural_default(eq_point(1, 2), eq_point(2, 1))}`
}

// An enum with an `eq` of its own: two shades are equal when as dark.
enum EqShade {
  Light
  Pale
  Dark
}
implement baml.ops.Equals for EqShade {
  function eq(self, other: Self) -> bool throws never {
    eq_darkness(self) == eq_darkness(other)
  }
}
function eq_darkness(shade: EqShade) -> int {
  match (shade) {
    EqShade.Light => 1,
    EqShade.Pale => 1,
    EqShade.Dark => 2,
  }
}
function eq_shade(n: int) -> EqShade {
  if (n == 0) { EqShade.Light } else if (n == 1) { EqShade.Pale } else { EqShade.Dark }
}
// `==` on two values of the enum is a call of its `eq`.
function eq_enum_own() -> string {
  `${eq_shade(0) == eq_shade(1)} ${eq_shade(0) == eq_shade(2)} ${eq_shade(1) != eq_shade(0)} ${eq_shade(2) != eq_shade(1)} ${eq_shade(2) == eq_shade(2)}`
}
// An array of `float`s compares as `float`s do where nothing says what it
// holds, which is so of a field declared `unknown`: a NaN equals itself,
// and the two zeros are equal. The arrays are made by compiled code, so
// only what it writes in their headers says they hold `float`s.
function eq_unknown_floats() -> string {
  let positive: float[] = [float.nan(), halves(0), halves(3)];
  let negative: float[] = [float.nan(), halves(0) * (0.0 - 1.0), halves(3)];
  let changed: float[] = [float.nan(), halves(1), halves(3)];
  let pushed: float[] = [];
  pushed.push(float.nan());
  pushed.push(halves(0) * (0.0 - 1.0));
  pushed.push(halves(3));
  `${EqLoose { held: positive } == EqLoose { held: negative }} ${EqLoose { held: positive } == EqLoose { held: changed }} ${EqLoose { held: positive } == EqLoose { held: pushed }}`
}
function eq_unknown_ints() -> string {
  let counted: int[] = [rep("a", 1).length(), 2];
  let written: int[] = [1, rep("a", 2).length()];
  let changed: int[] = [1, 3];
  let one: unknown = counted;
  let other: unknown = written;
  let third: unknown = changed;
  `${one == other} ${one == third}`
}
class EqLoose {
  held unknown
}
function eq_floats_made_by_natives() -> string {
  let source: float[] = [halves(3), float.nan(), halves(0)];
  let negative: float[] = [halves(3), float.nan(), halves(0) * (0.0 - 1.0)];
  let both = source.concat(negative);
  let ones: float[] = baml.Array.filled(2, float.nan());
  `${EqLoose { held: source.slice(1, 3) } == EqLoose { held: negative.slice(1, 3) }} ${EqLoose { held: both.slice(0, 3) } == EqLoose { held: both.slice(3, 6) }} ${EqLoose { held: ones } == EqLoose { held: [float.nan(), float.nan()] }} ${EqLoose { held: source.reverse() } == EqLoose { held: negative.reverse() }}`
}
function eq_floats_of_a_map() -> bool {
  let one: map<string, float> = { rep("a", 1): float.nan(), rep("b", 1): halves(0) };
  let other: map<string, float> = { rep("a", 1): float.nan(), rep("b", 1): halves(0) * (0.0 - 1.0) };
  EqLoose { held: one.values() } == EqLoose { held: other.values() }
}
function eq_floats_decoded() -> bool {
  let one = baml.json.decode<float[]>(rep("[0.0, 1.5]", 1));
  let other = baml.json.decode<float[]>(rep("[-0.0, 1.5]", 1));
  EqLoose { held: one } == EqLoose { held: other }
}
function eq_float_values_of_maps() -> string {
  let one: map<string, float> = { rep("a", 1): float.nan(), rep("b", 1): halves(0) };
  let other: map<string, float> = { rep("a", 1): float.nan(), rep("b", 1): halves(0) * (0.0 - 1.0) };
  let decoded = baml.json.decode<map<string, float>>(rep("{\"a\": 1.5, \"b\": -0.0}", 1));
  let written: map<string, float> = { rep("a", 1): halves(3), rep("b", 1): halves(0) };
  `${EqLoose { held: one } == EqLoose { held: other }} ${EqLoose { held: decoded } == EqLoose { held: written }} ${EqLoose { held: one } == EqLoose { held: written }}`
}

// A map with `float` keys keeps every NaN as one key and the two zeros as
// one, so two maps compare key by key whatever each was written with, and
// whether or not anything says what the keys are. A NaN negated is a NaN
// of other bits.
function eq_nan(negative: bool) -> float {
  let nan = float.nan();
  if (negative) { -nan } else { nan }
}
function eq_zero(negative: bool) -> float {
  let zero = halves(0);
  if (negative) { -zero } else { zero }
}
function eq_float_keyed(special: float, other: int) -> map<float, int> {
  let m: map<float, int> = {};
  m[special] = 1;
  m[halves(other)] = 2;
  m
}
function eq_nan_keyed_maps() -> string {
  let one = eq_float_keyed(eq_nan(false), 5);
  let other = eq_float_keyed(eq_nan(true), 5);
  let changed = eq_float_keyed(eq_nan(false), 6);
  `${one == other} ${one == changed} ${other[eq_nan(false)]} ${one[eq_nan(true)]}`
}
function eq_zero_keyed_maps() -> string {
  let one = eq_float_keyed(eq_zero(false), 5);
  let other = eq_float_keyed(eq_zero(true), 5);
  let changed = eq_float_keyed(eq_zero(false), 6);
  `${one == other} ${one == changed} ${other[eq_zero(false)]} ${one[eq_zero(true)]}`
}
function eq_nan_keyed_maps_unknown() -> string {
  let one = eq_float_keyed(eq_nan(false), 5);
  let other = eq_float_keyed(eq_nan(true), 5);
  let changed = eq_float_keyed(eq_nan(false), 6);
  `${EqLoose { held: one } == EqLoose { held: other }} ${EqLoose { held: other } == EqLoose { held: one }} ${EqLoose { held: one } == EqLoose { held: changed }}`
}
function eq_zero_keyed_maps_unknown() -> string {
  let one = eq_float_keyed(eq_zero(false), 5);
  let other = eq_float_keyed(eq_zero(true), 5);
  let changed = eq_float_keyed(eq_zero(false), 6);
  `${EqLoose { held: one } == EqLoose { held: other }} ${EqLoose { held: other } == EqLoose { held: one }} ${EqLoose { held: one } == EqLoose { held: changed }}`
}
// The second write of a NaN or of a zero is of the key already there, and
// a map written as a literal keeps its keys the same way.
function eq_float_keys_written_twice() -> string {
  let nans = eq_float_keyed(eq_nan(false), 5);
  nans[eq_nan(true)] = 10;
  let zeros = eq_float_keyed(eq_zero(false), 5);
  zeros[eq_zero(true)] = 10;
  let nan_literal: map<float, int> = { eq_nan(true): 10, halves(5): 2 };
  let zero_literal: map<float, int> = { eq_zero(true): 10, halves(5): 2 };
  `${nans.length()} ${zeros.length()} ${nans == nan_literal} ${zeros == zero_literal} ${EqLoose { held: nans } == EqLoose { held: nan_literal }} ${EqLoose { held: zeros } == EqLoose { held: zero_literal }}`
}
