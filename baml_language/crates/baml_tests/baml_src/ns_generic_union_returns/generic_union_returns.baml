class FinalValue {
  value string
}

class PendingCalls {
  calls string[]
}

class ModelStep<T> {
  outcome T | PendingCalls
}

class ParsedChild {
  name string
}

enum ParsedStatus {
  Ready
}

class ParsedNullableContainers {
  empty_json json[]?
  metadata map<string, json>?
  children ParsedChild[]?
  statuses ParsedStatus[]?
}

function parse_model_step<T>(raw string) -> ModelStep<T> throws unknown {
  ModelStep<T> {
    outcome: baml.sap.parse<T | PendingCalls>(raw),
  }
}

function model_step_flattens_union_type_arg() -> bool throws unknown {
  let step = parse_model_step<FinalValue | string>(`"done"`);
  match (step.outcome) {
    let value: string => value == "done",
    let value: FinalValue => value.value == "never",
    let calls: PendingCalls => calls.calls.length() == 99,
  }
}

function sap_parses_optional_json_null() -> bool throws unknown {
  let parsed = baml.sap.parse<json?>(`null`);
  match (parsed) {
    null => true,
    let value: json => baml.json.stringify(value) == "never",
  }
}

function sap_parses_optional_json_value() -> bool throws unknown {
  let parsed = baml.sap.parse<json?>(`true`);
  baml.json.stringify(parsed) == "true"
}

function sap_nullable_container_types_are_preserved() -> bool throws unknown {
  let parsed = baml.sap.parse<ParsedNullableContainers>(
    `{
      "empty_json": [],
      "metadata": {"attempt": 1},
      "children": [{"name": "first"}],
      "statuses": ["Ready"]
    }`
  );
  // These annotations are the static regression: SAP must preserve each
  // declared container type through the nullable wrapper.
  let empty_json: json[]? = parsed.empty_json;
  let metadata: map<string, json>? = parsed.metadata;
  let children: ParsedChild[]? = parsed.children;
  let statuses: ParsedStatus[]? = parsed.statuses;
  let empty_ok = (empty_json?.length() ?? -1) == 0;
  let map_ok = (metadata?.length() ?? 0) == 1;
  let children_ok = children?.at(0)?.name == "first";
  let statuses_ok = statuses?.at(0) == ParsedStatus.Ready;
  empty_ok && map_ok && children_ok && statuses_ok
}
