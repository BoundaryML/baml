// Maps of known value type read and written in place, and generic callees
// called plain, against the interpreter.
class KcPoint { x int y float }
class KcPair<T> { a T b T }

function kc_pick<T>(p: KcPair<T>, first: bool) -> T { if (first) { p.a } else { p.b } }
function kc_second<T>(xs: T[]) -> T { xs[1] }

function KcMaps() -> string {
  let ints: map<string, int> = {};
  let floats: map<string, float> = {};
  let points: map<string, KcPoint> = {};
  let maybe: map<string, int?> = {};
  let out: string[] = [];
  for (let k in ["a", "b", "a", "c", "b", "a"]) {
    ints[k] = (ints.get(k) ?? 0) + 1;
    floats[k] = (floats.get(k) ?? 0.5) * 2.0;
  }
  out.push((ints.set("a", 10) ?? -1).to_string());
  out.push((ints.set("z", 1) ?? -1).to_string());
  out.push((ints.get("missing") ?? -7).to_string());
  points["p"] = KcPoint { x: 1, y: 2.5 };
  let previous = points.set("p", KcPoint { x: 3, y: 0.25 });
  out.push(previous?.x.to_string() ?? "none");
  maybe["n"] = null;
  out.push(maybe.has("n").to_string() + ":" + (maybe.get("n") == null).to_string());
  let key = "ab".slice(0, 1);
  out.push((ints.get(key) ?? -1).to_string());
  out.push(baml.json.stringify(ints) + baml.json.stringify(floats) + points["p"].y.to_string());
  out.join("|")
}

function KcGenerics() -> string {
  let ints = KcPair<int> { a: 3, b: 4 };
  let names = KcPair<string> { a: "x", b: "y" };
  let points = KcPair<KcPoint> { a: KcPoint { x: 1, y: 1.0 }, b: KcPoint { x: 2, y: 2.0 } };
  let s = 0;
  for (let i = 0; i < 100; i += 1) { s += kc_pick(ints, i % 3 == 0); }
  s.to_string() + kc_pick(names, false) + kc_pick(points, false).x.to_string() + kc_second([1.5, 2.5]).to_string()
}

function KcGenericError() -> string {
  kc_second(["only"]) catch (e) {
    _ => "caught"
  }
}

function KcGenericUncaught() -> string {
  kc_second(["only"])
}
