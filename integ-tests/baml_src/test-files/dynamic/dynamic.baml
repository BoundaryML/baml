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

class DynInputOutput {
  testKey string
  @@dynamic
}

function DynamicInputOutput(input: DynInputOutput) -> DynInputOutput {
  client GPT35
  prompt #"
    Here is some input data:
    ----
    {{ input }}
    ----

    Extract the information.
    {{ ctx.output_format }}
  "#
}

function DynamicListInputOutput(input: DynInputOutput[]) -> DynInputOutput[] {
  client GPT35
  prompt #"
    Here is some input data:
    ----
    {{ input }}
    ----

    Extract the information.
    {{ ctx.output_format }}
  "#
}



class DynamicOutput {
  @@dynamic
}
 
function MyFunc(input: string) -> DynamicOutput {
  client GPT35
  prompt #"
    Given a string, extract info using the schema:

    {{ input}}

    {{ ctx.output_format }}
  "#
}

