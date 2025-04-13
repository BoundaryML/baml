type Primitive = int | string | bool | float

type List = string[]

type Graph = map<string, string[]>

type Combination = Primitive | List | Graph

function PrimitiveAlias(p: Primitive) -> Primitive {
    client "openai/gpt-4o"
    prompt r#"
      Return the given value back: {{ p }}

      {{ ctx.output_format }}
    "#
}

function MapAlias(m: Graph) -> Graph {
    client "openai/gpt-4o"
    prompt r#"
      Return the given Graph back:

      {{ m }}

      {{ ctx.output_format }}
    "#
}

function NestedAlias(c: Combination) -> Combination {
    client "openai/gpt-4o"
    prompt r#"
      Return the given value back:

      {{ c }}

      {{ ctx.output_format }}
    "#
}

// Test attribute merging.
type Currency = int @check(gt_ten, {{ this > 10 }})
type Amount = Currency @assert({{ this > 0 }})

class MergeAttrs {
    amount Amount @description("In USD")
}

// This should be allowed.
type MultipleAttrs = int @assert({{ this > 0 }}) @check(gt_ten, {{ this > 10 }})

function MergeAliasAttributes(money: int) -> MergeAttrs {
    client "openai/gpt-4o"
    prompt r#"
      Return the given integer in the specified format:

      {{ money }}

      {{ ctx.output_format }}
    "#
}

function ReturnAliasWithMergedAttributes(money: int) -> Amount {
    client "openai/gpt-4o"
    prompt r#"
      Return the given integer without additional context:

      {{ money }}

      {{ ctx.output_format }}
    "#
}

function AliasWithMultipleAttrs(money: int) -> MultipleAttrs {
    client "openai/gpt-4o"
    prompt r#"
      Return the given integer without additional context:

      {{ money }}

      {{ ctx.output_format }}
    "#
}

type RecursiveMapAlias = map<string, RecursiveMapAlias>

function SimpleRecursiveMapAlias(input: RecursiveMapAlias) -> RecursiveMapAlias {
    client "openai/gpt-4o"
    prompt r#"
      Return the given value:

      {{ input }}

      {{ ctx.output_format }}
    "#
}

type RecursiveListAlias = RecursiveListAlias[]

function SimpleRecursiveListAlias(input: RecursiveListAlias) -> RecursiveListAlias {
    client "openai/gpt-4o"
    prompt r#"
      Return the given JSON array:

      {{ input }}

      {{ ctx.output_format }}
    "#
}

type RecAliasOne = RecAliasTwo
type RecAliasTwo = RecAliasThree
type RecAliasThree = RecAliasOne[]

function RecursiveAliasCycle(input: RecAliasOne) -> RecAliasOne {
    client "openai/gpt-4o"
    prompt r#"
      Return the given JSON array:

      {{ input }}

      {{ ctx.output_format }}
    "#
}

type JsonValue = int | string | bool | float | JsonObject | JsonArray
type JsonObject = map<string, JsonValue>
type JsonArray = JsonValue[]

function JsonTypeAliasCycle(input: JsonValue) -> JsonValue {
    client "openai/gpt-4o"
    prompt r#"
      Return the given input back:

      {{ input }}

      {{ ctx.output_format }}
    "#
}

class RecursiveAliasDependency {
  value JsonValue
}

function TakeRecAliasDep(input: RecursiveAliasDependency) -> RecursiveAliasDependency {
    client "openai/gpt-4o"
    prompt r#"
      Return the given input back:

      {{ input }}

      {{ ctx.output_format }}
    "#
}
