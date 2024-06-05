class DynamicClassOne {
  @@dynamic
}

enum DynEnumOne {
  @@dynamic
}

enum DynEnumTwo {
  @@dynamic
}

class SomeClassNestedDynamic {
  hi string
  @@dynamic

}

class DynamicClassTwo {
  hi string
  some_class SomeClassNestedDynamic
  status DynEnumOne
  @@dynamic
}

function DynamicFunc(input: DynamicClassOne) -> DynamicClassTwo {
  client GPT35
  prompt #"
    Please extract the schema from 
    {{ input }}

    {{ ctx.output_format }}
  "#
}

class DynamicOutput {
  @@dynamic
}
 
function MyFunc(input: string) -> DynamicOutput {
  client GPT4
  prompt #"
    Given a string, extract info using the schema:

    {{ input}}

    {{ ctx.output_format }}
  "#
}