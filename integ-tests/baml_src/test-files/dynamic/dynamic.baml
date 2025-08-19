class DynamicClassOne {
  @@dynamic
}

enum DynEnumOne {
  @@dynamic
}

enum DynEnumTwo {
  @@dynamic
}

enum DynEnumThree {
  TRICYCLE @alias("bike with three wheels")
  TRIANGLE
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

function ClassifyDynEnumTwo(input: string) -> DynEnumTwo {
  client GPT35
  prompt #"
    Given a string, extract info using the schema:

    {{ input}}

    {{ ctx.output_format }}
  "#
}

function ClassifyDynamicStatus(input: string) -> DynEnumOne {
  client GPT35
  prompt #"
    Classify the status from the input text:

    {{ input }}

    {{ ctx.output_format }}
  "#
}

function ExtractDynamicCategories(input: string) -> DynEnumTwo[] {
  client GPT35
  prompt #"
    Extract all relevant categories from the input text:

    {{ input }}

    {{ ctx.output_format }}
  "#
}

