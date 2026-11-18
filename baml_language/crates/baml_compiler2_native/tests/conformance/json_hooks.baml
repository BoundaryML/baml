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
