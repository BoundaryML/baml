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
  `${eq_named(2, float.nan()) == eq_named(2, float.nan())} ${eq_named(2, 0.0 - halves(0)) == eq_named(2, halves(0))} ${eq_named(2, float.nan()) == eq_named(2, float.inf())}`
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
  let other: float[] = [float.nan(), 0.0 - halves(0), halves(3)];
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
