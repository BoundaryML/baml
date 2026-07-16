class FinalValue {
  value: string,
}

class PendingCalls {
  calls: string[],
}

class ModelStep<T> {
  outcome: T | PendingCalls,
}

class ParsedToolMessage {
  tool_calls: json[]?,
}

class ParsedChild {
  name: string,
}

class ParsedNullableContainers {
  empty_json: json[]?,
  metadata: map<string, json>?,
  children: ParsedChild[]?,
}

class ParsedOptionalJson {
  details: json?,
}

interface GenericUnionProvider {
  function explicit_step<T>(self, calls: string[]?, fallback: T) -> T | PendingCalls throws never
  function tail_step<T>(self, calls: string[]?, fallback: T) -> T | PendingCalls throws never
}

class GenericUnionProviderImpl {
}

implements GenericUnionProvider for GenericUnionProviderImpl {
  function explicit_step<T>(self, calls: string[]?, fallback: T) -> T | PendingCalls throws never {
    match (calls) {
      let pending: string[] => return PendingCalls { calls: pending },
      null => return fallback,
    }
  }

  function tail_step<T>(self, calls: string[]?, fallback: T) -> T | PendingCalls throws never {
    match (calls) {
      let pending: string[] => PendingCalls { calls: pending },
      null => fallback,
    }
  }
}

function generic_union_explicit_return_through_interface() -> bool throws never {
  let provider: GenericUnionProvider = GenericUnionProviderImpl {};
  let result = provider.explicit_step<FinalValue>(["search"], FinalValue { value: "done" });
  match (result) {
    let pending: PendingCalls => pending.calls == ["search"],
    let value: FinalValue => value.value == "never",
  }
}

function generic_union_tail_return_through_interface() -> bool throws never {
  let provider: GenericUnionProvider = GenericUnionProviderImpl {};
  let result = provider.tail_step<FinalValue>(["search"], FinalValue { value: "done" });
  match (result) {
    let pending: PendingCalls => pending.calls == ["search"],
    let value: FinalValue => value.value == "never",
  }
}

function generic_union_generic_branch_through_interface() -> bool throws never {
  let provider: GenericUnionProvider = GenericUnionProviderImpl {};
  let result = provider.tail_step<FinalValue>(null, FinalValue { value: "done" });
  match (result) {
    let pending: PendingCalls => pending.calls.length() == 99,
    let value: FinalValue => value.value == "done",
  }
}

function sap_parsed_json_array_branch<T>(fallback: T) -> T | PendingCalls throws unknown {
  let message = baml.sap.parse<ParsedToolMessage>(
    `{"tool_calls":[{"function":{"name":"search"}}]}`
  );
  match (message.tool_calls) {
    let calls: json[] => {
      let names: string[] = [];
      for (let call in calls) {
        names.push(baml.json.path_or(call, ".function.name", ""));
      }
      return PendingCalls { calls: names };
    },
    null => fallback,
  }
}

function parse_model_step<T>(raw: string) -> ModelStep<T> throws unknown {
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

function sap_parses_optional_json_class_field() -> bool throws unknown {
  let null_value = baml.sap.parse<ParsedOptionalJson>(`{"details": null}`);
  let object_value = baml.sap.parse<ParsedOptionalJson>(`{"details": {"ok": true}}`);
  let null_ok = match (null_value.details) {
    null => true,
    let value: json => false,
  };
  null_ok && baml.json.path_or<bool>(object_value.details, ".ok", false)
}

function generic_union_sap_parsed_json_array_branch() -> bool throws unknown {
  let result = sap_parsed_json_array_branch<FinalValue>(FinalValue { value: "done" });
  match (result) {
    let pending: PendingCalls => pending.calls == ["search"],
    let value: FinalValue => value.value == "never",
  }
}

function sap_nullable_container_types_are_preserved() -> bool throws unknown {
  let parsed = baml.sap.parse<ParsedNullableContainers>(
    `{
      "empty_json": [],
      "metadata": {"attempt": 1},
      "children": [{"name": "first"}]
    }`
  );
  let empty_ok = match (parsed.empty_json) {
    let values: json[] => values.length() == 0,
    null => false,
  };
  let map_ok = match (parsed.metadata) {
    let values: map<string, json> => baml.json.path_or(values, ".attempt", 0) == 1,
    null => false,
  };
  let children_ok = match (parsed.children) {
    let values: ParsedChild[] => values.at(0)?.name == "first",
    null => false,
  };
  empty_ok && map_ok && children_ok
}

test "generic union explicit return through an erased interface" {
  assert.is_true(generic_union_explicit_return_through_interface())
}

test "generic union tail return through an erased interface" {
  assert.is_true(generic_union_tail_return_through_interface())
}

test "generic union generic branch through an erased interface" {
  assert.is_true(generic_union_generic_branch_through_interface())
}

test "generic union concrete branch after SAP parsed json array" {
  assert.is_true(generic_union_sap_parsed_json_array_branch())
}

test "ModelStep flattens a union type argument before SAP conversion" {
  assert.is_true(model_step_flattens_union_type_arg())
}

test "SAP parses null as optional json" {
  assert.is_true(sap_parses_optional_json_null())
}

test "SAP parses a non-null optional json value" {
  assert.is_true(sap_parses_optional_json_value())
}

test "SAP parses optional json in a class field" {
  assert.is_true(sap_parses_optional_json_class_field())
}

test "SAP preserves declared types for nullable containers" {
  assert.is_true(sap_nullable_container_types_are_preserved())
}
