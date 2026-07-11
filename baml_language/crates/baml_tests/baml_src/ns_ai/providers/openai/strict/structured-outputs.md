# Structured return types

The BAML return type is the output contract. `OpenAiStrict` does not require a
class at the root because it wraps every result in an object property named
`value`.

## Supported shapes

| BAML return type | Strict `value` schema |
| --- | --- |
| `bool` | `{ "type": "boolean" }` |
| `string` | `{ "type": "string" }` |
| `Risk` | `{ "type": "string", "enum": [...] }` |
| `Incident` | closed object with all fields required |
| `A | B` | `anyOf` with definitions for both branches |
| `(A | B)[]` | one array whose `items` is that `anyOf` |
| recursive class | `$defs` and `$ref` preserve the cycle |
| optional field `string?` | required property whose type includes `null` |

These cases have both schema-shape tests and live `gpt-5.6-luna` coverage in
`ns_ai_scenarios/02_structured_output`.

## Primitive and enum roots

For `-> bool`, the provider sends:

```json
{
  "type": "function",
  "function": {
    "name": "__baml_return_output",
    "strict": true,
    "parameters": {
      "type": "object",
      "properties": {
        "value": { "type": "boolean" }
      },
      "required": ["value"],
      "additionalProperties": false
    }
  }
}
```

For a root enum:

```baml
enum Risk { Low Medium High }
```

the `value` property is:

```json
{ "type": "string", "enum": ["Low", "Medium", "High"] }
```

An enum field inside a class uses the same schema at that property. Both the
model arguments and BAML decoder therefore reject values outside the enum.

## Classes and optional fields

OpenAI strict schemas require closed object definitions. The adapter changes
each class object to:

```json
{
  "type": "object",
  "properties": {
    "name": { "type": "string" },
    "nickname": { "type": ["string", "null"] }
  },
  "required": ["name", "nickname"],
  "additionalProperties": false
}
```

This does not make `nickname` non-optional in BAML. It means the JSON key must
be present and its value may be `null`, which is OpenAI's strict-schema spelling
for an optional value.

## Unions

Given:

```baml
class Approved {
  kind: "approved",
  summary: string,
}

class Rejected {
  kind: "rejected",
  reason: string,
}

type Decision = Approved | Rejected
```

`-> Decision` produces one forced output call whose `value` is an `anyOf`.
Literal discriminator fields help both the model and BAML decoder choose the
correct branch.

## Lists of unions are not parallel tool calls

`-> Decision[]` produces:

```json
{
  "type": "array",
  "items": {
    "anyOf": [
      { "$ref": "#/$defs/Approved" },
      { "$ref": "#/$defs/Rejected" }
    ]
  }
}
```

The entire array is carried by one `__baml_return_output` call. This preserves
array order and validates the collection as a single return value.

`parallel_tool_calls` has a different meaning: it permits several independent
application function calls in one model turn. BAML must not enable it merely
because `T` happens to be an array.

## Recursive classes

For:

```baml
class TeamNode {
  name: string,
  risk: Risk,
  reports: TeamNode[],
}
```

the `reports.items` schema is a `$ref` and the complete closed definition is
placed in `$defs`. `OpenAiStrict` lifts `$defs` next to the wrapper's
`properties` so references below `value` still resolve:

```text
parameters
  properties
    value
      reports.items -> #/$defs/TeamNode
  $defs
    TeamNode
      reports.items -> #/$defs/TeamNode
```

## Standard schema first, provider transformation second

`baml.schema.json_schema(reflect.type_of<T>())` returns standard mutable JSON.
`root.ai.openai_strict_schema(...)` is a provider-local transformation that
closes objects and applies OpenAI's restrictions.

This boundary matters because other providers accept different subsets. BAML
types should not contain OpenAI quirks; each provider adapter should transform
or reject the standard schema explicitly.

See OpenAI's current [strict-mode requirements](https://developers.openai.com/api/docs/guides/function-calling#strict-mode)
and [Structured Outputs limitations](https://developers.openai.com/api/docs/guides/structured-outputs#supported-schemas).
