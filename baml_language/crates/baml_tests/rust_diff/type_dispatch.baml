function id<T>(x: T) -> T { x }
function recur<T>(x: T, n: int) -> T { if n == 0 { x } else { recur<T>(x, n - 1) } }
class Box<T> { value T }
class MetadataOnly {}
enum MetadataEnum { One Two }
class Phantom<T> { number int }
function phantom(p: Phantom<MetadataOnly>) -> int { p.number }
function phantom_enum(p: Phantom<MetadataEnum>) -> int { p.number }
interface Get<T> { function get(self) -> T throws never }
class Wrap<T> {
  value T
  implements Get<T> { function get(self) -> T throws never { self.value } }
}
interface Default { function amount(self) -> int throws never { 17 } }
class Empty { implements Default {} }
interface Score { function score(self) -> int throws never }
implements Score for int { function score(self) -> int throws never { self + 10 } }
implements Score for string { function score(self) -> int throws never { self.length() } }
function score(x: Score) -> int { x.score() }
function read(x: Get<int>) -> int { x.get() }
function defaulted(x: Default) -> int { x.amount() }
enum Choice { A B C }
function choice() -> Choice { Choice.C }
function choose(e: Choice) -> int { match(e) { Choice.A => 2, Choice.B => 3, Choice.C => 5 } }
function maybe(x: int) -> int? { match(x) { 3 => 7, 9 => 11, _ => null } }
function classify(x: int | string | bool) -> int { match(x) { int => 13, string => 19, bool => 23 } }
type Tree = int | Tree[]
class Node { tree Tree }

interface Slot<T> { value: T }
class Pair {
  number: int
  text: string
  implements Slot<int> { value as number }
  implements Slot<string> { value as text }
}
function view<T>(p: Slot<T>) -> T { p.value }
function bump(p: Slot<int>) -> int { p.value = p.value + 1; p.value }
interface Rank { function rank(self) -> int throws never }
class R1 { implements Rank { function rank(self) -> int throws never { 1 } } }
class R2 { implements Rank { function rank(self) -> int throws never { 2 } } }
class R3 { implements Rank { function rank(self) -> int throws never { 3 } } }
function rank(r: Rank) -> int { r.rank() }
implements Score for bool { function score(self) -> int throws never { if self { 5 } else { 7 } } }
implements Score for float { function score(self) -> int throws never { if self > 1.0 { 11 } else { 13 } } }
implements Score for int[] { function score(self) -> int throws never { 17 } }
implements Score for map<string,int> { function score(self) -> int throws never { 19 } }
implements Score for Choice { function score(self) -> int throws never { choose(self) } }
function ints() -> int[] { [1,2] }
function texts() -> string[] { ["no"] }
function tree(value: unknown) -> int { if value is Tree { 23 } else { 29 } }
interface Identity { function identity<T>(self, x: T) -> T throws never }
class Ident { implements Identity { function identity<T>(self, x: T) -> T throws never { x } } }
function identity(x: Identity) -> int { x.identity<int>(31) }
class NamingField { naming baml.json.Naming }
function naming(n: baml.json.Naming) -> int {
  match(n) { baml.json.Naming.Aliased => 43, baml.json.Naming.Declared => 47 }
}
interface Tagged { function tag(self) -> int throws never }
implements<T> Tagged for Box<T> { function tag(self) -> int throws never { 53 } }
function tag(value: Tagged) -> int { value.tag() }
type Scalar = int | string | null
class AliasField { value Scalar }
function scalar(value: Scalar) -> int {
  match(value) { int => 59, string => 61, null => 67 }
}
interface AliasDispatch { function amount(self) -> int throws never }
class AliasOwner<T> { left: int, right: T }
type PinnedOwner = AliasOwner<string>
implement AliasDispatch for PinnedOwner {
  function amount(self) -> int throws never { self.left }
}
function alias_dispatch(value: AliasDispatch) -> int { value.amount() }
class JsonOverride {
  value: int
  implements baml.ToJson {
    function to_json(self) -> json throws baml.json.SerializationError { "custom" }
  }
}
function explicit_json_override() -> int {
  let value = JsonOverride { value: 1 }.to_json();
  match (value) { string => value.length(), _ => 0 }
}
interface ProjectionSource { type Value }
class ProjectionInt { implements ProjectionSource { type Value = int } }
class ProjectionBool { implements ProjectionSource { type Value = bool } }
class ProjectionField<T extends ProjectionSource> { value T.Value }
function projection_flag(p: ProjectionField<ProjectionBool>) -> int {
  match (p) {
    ProjectionField<ProjectionBool> { value: true } => 1,
    ProjectionField<ProjectionBool> { value: false } => 0,
  }
}
function projection_fields() -> int {
  let p: ProjectionField<ProjectionInt> = ProjectionField { value: 127 };
  let yes: ProjectionField<ProjectionBool> = ProjectionField { value: true };
  let no: ProjectionField<ProjectionBool> = ProjectionField { value: false };
  p.value + projection_flag(yes) + projection_flag(no)
}
interface HasProjectedError {
  type E
  function read(self) -> int throws Self.E
}
interface ReadMany {
  type E
  function read_many(self) -> int throws Self.E
}
implement<T extends HasProjectedError> ReadMany for T[] {
  type E = T.E
  function read_many(self) -> int throws T.E { self[0].read() }
}
class NonThrow {
  amount: int
  implements HasProjectedError {
    type E = never
    function read(self) -> int throws never { self.amount }
  }
}
function projection_error() -> int throws never {
  let xs = [NonThrow { amount: 131 }];
  xs.read_many()
}
class ProjectedFailure { amount: int }
class ThrowingRead {
  implements HasProjectedError {
    type E = ProjectedFailure
    function read(self) -> int throws ProjectedFailure {
      throw ProjectedFailure { amount: 137 }
    }
  }
}
function projection_catch() -> int {
  let xs = [ThrowingRead {}];
  xs.read_many() catch (e) { ProjectedFailure => e.amount }
}
class DecodedBox<T> { value: T }
class DecodedPair { ints: DecodedBox<int>, texts: DecodedBox<string> }
type DecodedAlias = DecodedBox<int> | DecodedAlias[]
function decoded_generics() -> int {
  let pair = baml.json.decode<DecodedPair>(`{"ints":{"value":139},"texts":{"value":"generic"}}`);
  let decoded = baml.json.decode<DecodedBox<int>>(`{"value":149}`);
  let list = baml.json.decode<DecodedBox<int>[]>(`[{"value":151}]`);
  let alias = baml.json.decode<DecodedAlias>(`[{"value":157}]`);
  let from_alias = match (alias) {
    let value: DecodedBox<int> => value.value,
    let values: DecodedAlias[] => match (values[0]) { let item: DecodedBox<int> => item.value, _ => 0 },
  };
  pair.ints.value + pair.texts.value.length() + decoded.value + list[0].value + from_alias
}
class EffectCalculator {
  base: int
  function add(self, n: int) -> int { self.base + n }
}
class EffectHolder { callback: (int) -> int throws never }
function apply_effect(f: (int) -> int, n: int) -> int { f(n) }
function apply_generic_effect<T>(f: (T) -> T, n: T) -> T { f(n) }
function fail_bottom() -> never { baml.sys.panic("bottom") }
function bottom_result() -> int {
  { fail_bottom(); 0 } catch (e) { baml.panics.UserPanic => 97 }
}
function effect_result() -> int {
  let calc = EffectCalculator { base: 89 };
  let holder = EffectHolder { callback: calc.add };
  let applied = apply_generic_effect<int>;
  applied(holder.callback, 3) + apply_effect(holder.callback, 1) + apply_generic_effect<int>(holder.callback, 2)
}
interface OptSpeaker { function speak(self) -> int throws never; }
class OptDog {
  implements OptSpeaker { function speak(self) -> int throws never { 163 } }
}
interface NeverBuilt { function make(self) -> int throws never; }
function opt_speak(a: OptSpeaker?) -> int {
  match (a) { let s: OptSpeaker => s.speak(), null => 1 }
}
function never_built(a: NeverBuilt?) -> int {
  match (a) { let s: NeverBuilt => s.make(), null => 167 }
}
class HashOnlyEq {
  value: int
  implements baml.ops.Equals { function eq(self, other: Self) -> bool throws never { true } }
}
function hashable(value: unknown) -> int { if (value is baml.Hash) { 1 } else { 0 } }
function optional_dispatch() -> int {
  let dog: OptSpeaker? = OptDog {};
  opt_speak(dog) + opt_speak(null) + never_built(null)
    + hashable(1) * 2 + hashable([1, 2]) * 4 + hashable(HashOnlyEq { value: 1 }) * 8
}
type AliasKey = string[];
type AliasMap = map<string, int>;
type AliasBox = Box<int>;
type AliasKeyAgain = AliasKey;
const ALIAS_GLOBAL: AliasKey = ["g", "h"];
class AliasHolder { key: AliasKey, count: AliasMap }
function alias_len(k: AliasKeyAgain) -> int { k.length() }
function alias_values() -> int {
  let k: AliasKey = ["a", "bc"];
  let m: AliasMap = { "x": 3 };
  let b: AliasBox = Box<int> { value: 5 };
  let holder = AliasHolder { key: k, count: m };
  ALIAS_GLOBAL.join(".").length() + k[1].length() + alias_len(k) + m["x"] + b.value
    + holder.key.length() + holder.count.length()
}
function main() -> int {
  let a = Box<int> { value: 29 };
  let b = Box<string> { value: "hello" };
  let n = Node { tree: 31 };
  let total = recur<int>(id<int>(a.value), 3) + id<string>(b.value).length();
  total += read(Wrap<int> { value: 37 }) + defaulted(Empty {});
  total += score(41) + score("abc");
  total += choose(Choice.B) + classify("text") + classify(4) + classify(true);
  total += (maybe(3) ?? 0) + (maybe(4) ?? 0);
  let p = Pair { number: 37, text: "pair" };
  total += view<int>(p) + view<string>(p).length() + bump(p);
  total += rank(R1 {}) + rank(R2 {}) + rank(R3 {});
  total += score(true) + score(1.5) + score(ints()) + score(choice());
  let m: map<string,int> = {};
  total += score(m) + tree(ints()) + tree(texts());
  total += identity(Ident {}) + tree(id<Tree>(n.tree));
  total += naming(NamingField { naming: baml.json.Naming.Aliased }.naming);
  total += naming(baml.json.Naming.Declared) + tag(a) + tag(b);
  total += scalar(AliasField { value: null }.value) + scalar(1) + scalar("x");
  total += phantom(Phantom<MetadataOnly> { number: 79 });
  total += phantom_enum(Phantom<MetadataEnum> { number: 83 });
  total += effect_result() + bottom_result() + explicit_json_override();
  total += projection_fields() + projection_error();
  total += projection_catch() + decoded_generics();
  total += alias_dispatch(AliasOwner<string> { left: 101, right: "alias" });
  total += optional_dispatch() + alias_values();
  total
}
