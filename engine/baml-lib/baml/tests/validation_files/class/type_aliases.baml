type Primitive = int | string | bool | float

type List = string[]

type Graph = map<string, string[]>

type Combination = Primitive | List | Graph

// Alias with attrs.
type Currency = int @check(gt_ten, {{ this > 10 }})
type Amount = Currency @assert({{ this > 0 }})

// Should be allowed.
type MultipleAttrs = int @assert({{ this > 0 }}) @check(gt_ten, {{ this > 10 }})

enum SomeEnum {
    A
    B
    C
}

type EnumAlias = SomeEnum

class TypeAliasPointsToEnum {
    enm EnumAlias
}

class MergeAttrs {
    amount Amount @description("In USD")
}

function PrimitiveAlias(p: Primitive) -> Primitive {
    client "openai/gpt-4o"
    prompt #"
      Return the given value back: {{ p }}
    "#
}

function MapAlias(m: Graph) -> Graph {
    client "openai/gpt-4o"
    prompt #"
      Return the given Graph back:

      {{ m }}

      {{ ctx.output_format }}
    "#
}

function NestedAlias(c: Combination) -> Combination {
    client "openai/gpt-4o"
    prompt #"
      Return the given value back:

      {{ c }}

      {{ ctx.output_format }}
    "#
}

function MergeAliasAttributes(money: int) -> MergeAttrs {
    client "openai/gpt-4o"
    prompt #"
      Return the given integer in the specified format:

      {{ money }}

      {{ ctx.output_format }}
    "#
}
