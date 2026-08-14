class PatternRecord {
  id int
  label string
}

class PatternEnvelope {
  record PatternRecord
  tags string[]
}

class Destructure {
  field string
}

type PatternInput = string | int | float | bool | null | PatternRecord | PatternEnvelope | Destructure | string[]
type PatternCallback = (int) -> int;

function classify_status(status: string, fallback: string?) -> string {
  match status {
    "accept" => "accepted",
    "retry" if env.SHOULD_RETRY => "retrying",
    _ => match (fallback) {
      null => "missing fallback",
      _ => "fallback",
    },
  }
}

function classify_input(input: PatternInput) -> string {
  match (input) {
    string => "text",
    int | float => "number",
    bool => {
      let matched = input;
    },
    null => "empty",
    PatternRecord? => "record-ish",
    root.matching.PatternRecord => "namespaced record",
    map<string, PatternRecord[]> => "record map",
    PatternRecord { id, label } => label,
    PatternRecord { id: let record_id, label: string } => record_id,
    let x: let y: let z: Destructure { field } => field,
    let PatternEnvelope { record: PatternRecord { label: let name }, tags } => name,
    name.space.PatternEnvelope { record } => "namespaced destructure",
    [let first, string, ..let rest]: string[] => first,
    let [let head, ..let tail]: string[] => head,
    [..] => "rest only",
    _ => "unknown",
  }
}

function bind_in_block(input: PatternInput) -> string {
  let value = input;
  let text: string = "value";
  let rendered = root.format(input, baml.defaults.label);
  let converted = root.convert<map<string, PatternInput>, string>(input);
  let recovered = root.fetch(input) catch (e) {
    _ => {
      let fallback = baml.default_value();
      fallback
    }
  }
  let Destructure { field } = input
  let [let first, ..let rest]: string[] = ["value"]
  let x: let y: let z: Destructure { field } = input
  if let Destructure { field } = input {
    return field;
  } else if root.has_fallback(input) {
    return root.fallback(input);
  } else {
    return baml.default_value();
  }
  if let Destructure { field } = input {
    return field;
  } else {
    return baml.default_value();
  }
  if rendered != "" {
    return rendered;
  }
}

function loop_patterns(inputs: PatternInput[], rows: string[][]) -> string {
  let rendered = "";
  for (let item in inputs) {
    if let Destructure { field } = item {
      rendered += field;
    } else {
      continue;
    }
  }
  for let [let first, ..let rest]: string[] in rows {
    rendered += first;
  }
  for (let i = 0; i < 3; i += 1) {
    rendered += "!";
  }
  while (rendered != "") {
    break;
  }
  while let Destructure { field } = root.next(inputs) {
    rendered += field;
    break;
  }
  return rendered;
}

function classify_binding(input: PatternInput, callback: PatternCallback) -> string {
  match (input) {
    let text: string => text,
    let record: PatternRecord => record.label,
    (let n: int) | (let n: int) => "integer",
    let value => "bound fallback",
    let _ => "wildcard binding",
  }

  match (callback) {
    let f: (int) -> int => "callable",
    _ => "not callable",
  }
}
