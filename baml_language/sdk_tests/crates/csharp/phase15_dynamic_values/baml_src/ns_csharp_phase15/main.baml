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
