// A class's own `to_json`: `baml.json` calls it wherever it meets an
// instance, at any depth, and writes what it returns in the instance's
// place. The native that does so is in Rust, and the `to_json` it calls is
// compiled BAML that allocates, calls `baml.json` again, throws and waits.

class HookTemp {
  celsius: float
  implements baml.ToJson {
    function to_json(self) -> baml.json.json throws baml.json.SerializationError {
      { "c": baml.json.from(self.celsius), "f": baml.json.from(self.celsius * 1.8 + 32.0) }
    }
  }
}
class HookTrip {
  name: string
  temps: HookTemp[]
  by_day: map<string, HookTemp>
  first: HookTemp?
  none: HookTemp?
}
class HookPlain {
  label: string
  temp: HookTemp
}
// It implements the interface and leaves `to_json` to the default.
class HookDefault {
  label: string
  temp: HookTemp
  implements baml.ToJson {}
}
// Its `to_json` calls `baml.json` again, on a value with a `to_json`.
class HookNested {
  trip: HookTrip
  implements baml.ToJson {
    function to_json(self) -> baml.json.json throws baml.json.SerializationError {
      { "text": baml.json.from(baml.json.stringify(self.trip)), "size": baml.json.from(self.trip.temps.length()) }
    }
  }
}
// Its `to_json` throws what a `to_json` may, and panics.
class HookFails {
  code: int
  implements baml.ToJson {
    function to_json(self) -> baml.json.json throws baml.json.SerializationError {
      if self.code > 100 { return baml.json.from(1000 / (self.code - self.code)); }
      if self.code > 0 {
        throw baml.json.SerializationError { message: rep("m", self.code), path: rep("p", 2), reason: "hook" }
      }
      baml.json.from(self.code)
    }
  }
}
class HookTally { calls: int }
class HookCounted {
  tally: HookTally
  implements baml.ToJson {
    function to_json(self) -> baml.json.json throws baml.json.SerializationError {
      self.tally.calls += 1;
      baml.json.from(self.tally.calls)
    }
  }
}
class HookCountedThenBad {
  counted: HookCounted[]
  bad: float
  after: HookCounted
}
class HookSlow {
  milliseconds: int
  implements baml.ToJson {
    function to_json(self) -> baml.json.json throws baml.json.SerializationError {
      baml.sys.sleep(baml.time.Duration.from_milliseconds(self.milliseconds));
      baml.json.from(rep("z", self.milliseconds))
    }
  }
}

function hook_temp(i: int) -> HookTemp { HookTemp { celsius: halves(i) } }
function hook_trip(n: int) -> HookTrip {
  let temps: HookTemp[] = [];
  let by_day: map<string, HookTemp> = {};
  for (let i = 0; i < n; i += 1) {
    temps.push(hook_temp(i));
    by_day[`day${i}`] = hook_temp(i * 3);
  };
  HookTrip { name: rep("t", n), temps: temps, by_day: by_day, first: hook_temp(n), none: null }
}

function hook_at_the_root() -> string {
  baml.json.stringify(hook_temp(5))
}
function hook_at_the_root_of_from() -> string {
  let j = baml.json.from(hook_temp(4));
  `${baml.json.path<float>(j, "f")} ${baml.json.stringify(j)}`
}
function hook_in_arrays_maps_and_classes() -> string {
  baml.json.stringify(hook_trip(4))
}
function hook_in_arrays_maps_and_classes_indented() -> string {
  baml.json.stringify(hook_trip(2), indent = 2)
}
function hook_in_from() -> string {
  let j = baml.json.from(hook_trip(3));
  `${baml.json.path<float>(j, "temps[2].f")} ${baml.json.path<float>(j, "by_day.day1.c")} ${baml.json.stringify(j)}`
}
function hook_in_an_unknown_value() -> string {
  let values: (HookTemp | HookPlain | string)[] = [hook_temp(1), HookPlain { label: rep("p", 2), temp: hook_temp(2) }, rep("s", 3)];
  baml.json.stringify(values)
}
// Enough of them that each call of `to_json` collects, with the values
// still to be written and the results so far held by the native.
function hook_many() -> int {
  let temps: HookTemp[] = [];
  for (let i = 0; i < 200; i += 1) { temps.push(hook_temp(i)); };
  baml.json.stringify(temps).length()
}
function hook_calls_json_again() -> string {
  baml.json.stringify([HookNested { trip: hook_trip(2) }, HookNested { trip: hook_trip(1) }])
}
// The default `to_json`, of a class that has none of its own, calls the
// `to_json` of what the class holds.
function hook_under_the_default_to_json() -> string {
  let plain = HookDefault { label: rep("p", 3), temp: hook_temp(7) };
  baml.json.stringify(baml._to_json_default(plain))
}
function hook_called_directly() -> string {
  baml.json.stringify(hook_temp(3).to_json())
}
function hook_throws_and_is_caught_by_class() -> int {
  let fails = [HookFails { code: task_count(0) }, HookFails { code: task_count(7) }];
  let text = baml.json.stringify(fails) catch (e) {
    baml.json.SerializationError => { return e.message.length() * 10 + e.path.length(); },
  };
  text.length()
}
function hook_throws_and_is_not_caught() -> string {
  baml.json.stringify({ "a": HookFails { code: task_count(3) } })
}
function hook_panics_and_is_caught_by_class() -> int {
  let text = baml.json.stringify(baml.json.from([HookFails { code: task_count(101) }])) catch (e) {
    baml.json.SerializationError => { return -1; },
    baml.panics.DivisionByZero => { return 77; },
  };
  text.length()
}
// The trace of what a `to_json` threw names the `to_json`, read where a
// handler outside the native takes the error.
function hook_throw_keeps_its_trace() -> string {
  let text = baml.json.stringify(HookFails { code: task_count(2) }) catch (e, context) {
    _ => {
      let names: string[] = [];
      for (let i = 0; i < context.stack_trace.frames.length(); i += 1) { names.push(context.stack_trace.frames[i].function_name); };
      return names.join(" < ");
    },
  };
  text
}
// Every `to_json` runs before a value is refused: the count is of all
// three, and the error is the refusal.
function hook_all_run_before_a_refusal() -> string {
  let tally = HookTally { calls: task_count(0) };
  let value = HookCountedThenBad {
    counted: [HookCounted { tally: tally }, HookCounted { tally: tally }],
    bad: halves(1) / halves(0),
    after: HookCounted { tally: tally },
  };
  let text = baml.json.stringify(value) catch (e) {
    baml.json.SerializationError => `${e.message} at ${e.path}`,
  };
  `${tally.calls} ${text}`
}
// A `to_json` that waits: the task is parked inside the native, and other
// tasks run and collect meanwhile.
function hook_parks() -> string {
  let slow = spawn { baml.json.stringify([HookSlow { milliseconds: task_count(30) }, HookSlow { milliseconds: task_count(5) }]) };
  let busy = spawn { task_churn(60) + baml.json.stringify(hook_trip(3)).length() };
  let quick = spawn { baml.json.stringify(HookSlow { milliseconds: task_count(1) }) };
  let n = await busy;
  `${n} ${await quick} ${(await slow).length()}`
}
function hook_parks_and_then_throws() -> int {
  let f = spawn {
    baml.json.stringify([baml.json.from(HookSlow { milliseconds: task_count(10) }), baml.json.from(HookFails { code: task_count(6) })])
  };
  let churned = task_churn(30);
  let text = (await f) catch (e) {
    baml.json.SerializationError => { return e.message.length() + churned * 0; },
  };
  text.length()
}

// A class's own `from_json`: `baml.json` calls it wherever the type it
// reads names the class, with the `json` in that place, and what it
// returns is the value.

class HookRange {
  low: int
  high: int
  implements baml.FromJson {
    function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
      let text = baml.json.to<string>(j);
      let parts = text.split("..");
      if parts.length() != 2 {
        throw baml.json.DecodeError { message: `not a range: ${text}`, path: "" }
      }
      HookRange { low: baml.Int.parse(parts[0]) catch (e) { _ => -1 }, high: baml.Int.parse(parts[1]) catch (e) { _ => -1 } }
    }
  }
}
class HookSpans {
  name: string
  first: HookRange
  rest: HookRange[]
  named: map<string, HookRange>
  maybe: HookRange?
  none: HookRange?
}
// Read by its fields: a member of a union is chosen by them, and then
// built by its `from_json`.
class HookTagged {
  kind: "tagged"
  size: int
  implements baml.FromJson {
    function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
      HookTagged { kind: "tagged", size: baml.json.to<int>(baml.json.field(j, "size")) * 100 }
    }
  }
}
class HookOther {
  kind: "other"
  label: string
}
class HookEither {
  items: (HookTagged | HookOther)[]
}
// Its `from_json` reads a document of its own with `baml.json`, which
// calls another `from_json`.
class HookEnvelope {
  spans: HookSpans
  implements baml.FromJson {
    function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
      let inner = baml.json.decode<HookSpans>(baml.json.to<string>(baml.json.field(j, "text"))) catch (e) {
        baml.json.ParseError => { throw baml.json.DecodeError { message: e.message, path: "text" } },
      };
      HookEnvelope { spans: inner }
    }
  }
}
class HookSlowRead {
  waited: int
  implements baml.FromJson {
    function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
      let milliseconds = baml.json.to<int>(j);
      baml.sys.sleep(baml.time.Duration.from_milliseconds(milliseconds));
      HookSlowRead { waited: milliseconds + task_churn(10) * 0 }
    }
  }
}
class HookCountedRead {
  seen: int
  implements baml.FromJson {
    function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
      let tally = baml.json.to<int[]>(j);
      HookCountedRead { seen: tally.length() }
    }
  }
}
class HookReadThenBad {
  before: HookRange
  bad: int
  after: HookRange
}

function hook_spans_doc(n: int) -> string {
  let rest: string[] = [];
  let named: string[] = [];
  for (let i = 0; i < n; i += 1) {
    rest.push(`"${i}..${i * 2}"`);
    named.push(`"k${i}": "${i * 10}..${i * 10 + 1}"`);
  };
  `{"name": "${rep("n", n)}", "first": "1..9", "rest": [${rest.join(", ")}], "named": {${named.join(", ")}}, "maybe": "7..8", "none": null}`
}

function hook_from_at_the_root() -> int {
  let range = baml.json.decode<HookRange>(rep("\"", 1) + "3..40" + rep("\"", 1));
  range.low * 1000 + range.high
}
function hook_from_at_the_root_of_to() -> int {
  let range = baml.json.to<HookRange>(baml.json.from(rep("12", 1) + ".." + rep("5", 2)));
  range.low * 1000 + range.high
}
function hook_from_in_arrays_maps_and_classes() -> string {
  let spans = baml.json.decode<HookSpans>(hook_spans_doc(4));
  let none = if spans.none == null { 1 } else { 0 };
  let maybe = match (spans.maybe) {
    null => -1,
    let range: HookRange => range.high,
  };
  `${spans.name} ${spans.first.high} ${spans.rest.length()} ${spans.rest[3].high} ${spans.named["k2"].low} ${maybe} ${none}`
}
// Enough of them that each `from_json` collects, with the values still to
// be read and the results so far held by the native.
function hook_from_many() -> int {
  let parts: string[] = [];
  for (let i = 0; i < 150; i += 1) { parts.push(`"${i}..${i + 1}"`); };
  let ranges = baml.json.decode<HookRange[]>("[" + parts.join(",") + "]");
  let sum = 0;
  for (let i = 0; i < ranges.length(); i += 1) { sum += ranges[i].high - ranges[i].low + ranges[i].low % 7; };
  sum
}
function hook_from_in_a_union_chosen_by_fields() -> string {
  let either = baml.json.decode<HookEither>("{\"items\": [{\"kind\": \"other\", \"label\": \"" + rep("l", 2) + "\"}, {\"kind\": \"tagged\", \"size\": 3}]}");
  let out: string[] = [];
  for (let i = 0; i < either.items.length(); i += 1) {
    out.push(match (either.items[i]) {
      let t: HookTagged => `tagged ${t.size}`,
      let o: HookOther => `other ${o.label}`,
    });
  };
  out.join(", ")
}
function hook_from_calls_json_again() -> string {
  let doc = baml.json.stringify({ "text": hook_spans_doc(2) });
  let envelope = baml.json.decode<HookEnvelope>(doc);
  `${envelope.spans.rest[1].high} ${envelope.spans.named["k1"].low}`
}
function hook_from_throws_and_is_caught_by_class() -> string {
  let ranges = baml.json.decode<HookRange[]>("[\"1..2\", \"" + rep("x", 3) + "\"]") catch (e) {
    baml.json.ParseError => { return "parse"; },
    baml.json.DecodeError => { return `${e.message} at <${e.path}>`; },
  };
  `${ranges.length()}`
}
function hook_from_throws_and_is_not_caught() -> int {
  baml.json.decode<HookSpans>("{\"name\": \"n\", \"first\": \"" + rep("1", 2) + "\", \"rest\": [], \"named\": {}}").first.low
}
// A value that does not fit is refused where the decoder meets it: the
// `from_json` before it has run, and the one after it has not.
function hook_from_runs_up_to_a_misfit() -> string {
  let read = baml.json.decode<HookReadThenBad>("{\"before\": \"1..2\", \"bad\": \"" + rep("b", 1) + "\", \"after\": \"oops\"}") catch (e) {
    baml.json.ParseError => { return "parse"; },
    baml.json.DecodeError => { return `${e.message} at <${e.path}>`; },
  };
  `${read.bad}`
}
function hook_from_throws_before_a_misfit() -> string {
  let read = baml.json.decode<HookReadThenBad>("{\"before\": \"" + rep("o", 2) + "\", \"bad\": \"b\", \"after\": \"1..2\"}") catch (e) {
    baml.json.ParseError => { return "parse"; },
    baml.json.DecodeError => { return `${e.message} at <${e.path}>`; },
  };
  `${read.bad}`
}
function hook_from_parks() -> string {
  let slow = spawn { baml.json.decode<HookSlowRead[]>("[" + rep("2", 2) + ", 4]") };
  let busy = spawn { task_churn(60) + baml.json.decode<HookSpans>(hook_spans_doc(3)).rest.length() };
  let quick = spawn { baml.json.to<HookSlowRead>(baml.json.from(task_count(1))) };
  let n = await busy;
  let read = await slow;
  `${n} ${(await quick).waited} ${read[0].waited} ${read[1].waited}`
}
// The default `from_json` reads the class by its fields, each field by
// its own `from_json`.
function hook_from_under_the_default_from_json() -> string {
  let spans = baml._from_json_structural_default<HookSpans>(baml.json.parse(hook_spans_doc(2)));
  let tally = baml._from_json_structural_default<HookCountedRead>(baml.json.parse("{\"seen\": " + rep("4", 1) + "}"));
  `${spans.rest[1].high} ${tally.seen}`
}
