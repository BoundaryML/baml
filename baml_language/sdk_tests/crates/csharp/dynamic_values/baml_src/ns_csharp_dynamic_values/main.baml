enum DynamicColor {
  red
  blue
}

type DynamicLabel = string

class DynamicRecord {
  label string
  count int
}

class DynamicBox<T> {
  value T
}

class DynamicChoice {
  value int | "fixed"
}

class DynamicEnvelope {
  color DynamicColor
  record DynamicRecord
  boxed DynamicBox<int>
  choice int | "fixed"
}

class DynamicNode {
  name string
  next DynamicNode?
}

function echo_unknown(value: unknown) -> unknown {
  value
}

function echo_unknown_list(value: unknown[]) -> unknown[] {
  value
}

function echo_unknown_map(
  value: map<string, unknown>,
) -> map<string, unknown> {
  value
}

function echo_choice(value: DynamicChoice) -> DynamicChoice {
  value
}

function echo_box(value: DynamicBox<int>) -> DynamicBox<int> {
  value
}

function touch_before_dispatch(
  probe: () -> int,
  value: unknown,
) -> unknown {
  probe()
  value
}

function touch_string_before_dispatch(
  probe: () -> int,
  value: string,
) -> string {
  probe()
  value
}

// Two type values naming one runtime declaration, and one type value naming
// it twice (`Widget | Widget[]`): the engine keeps one key per declaration,
// so every occurrence carries the same key.
function runtime_type_twice() -> unknown {
  let widget = reflect.Class.new("Widget", { "value": reflect.Type.of<int>() }).as_type()
  [widget, widget, reflect.Union.new([widget, widget.array()]).as_type()]
}
