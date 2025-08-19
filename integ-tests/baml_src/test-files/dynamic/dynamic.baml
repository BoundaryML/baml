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

function RenderDynamicEnum(tricycle: DynEnumThree, other: DynEnumThree) -> string {
  client GPT35
  prompt #"
    "DynEnumThree.TRICYCLE" renders as: {{ tricycle }}
    "other" renders as: {{ other }}
    
    Available dynamic enum values:
      - TRICYCLE: {{ DynEnumThree.TRICYCLE }}
      - TRIANGLE: {{ DynEnumThree.TRIANGLE }}

    Enum comparison tests:

    {% if tricycle == DynEnumThree.TRICYCLE %}
    DynEnumThree matches TRICYCLE enum value, as expected
    {% else %}
    DynEnumThree should match TRICYCLE enum value, but does not
    {% endif %}

    {% if tricycle != DynEnumThree.TRIANGLE %}
    DynEnumThree is not TRIANGLE, as expected
    {% else %}
    DynEnumThree should not be TRIANGLE, but it is
    {% endif %}

    {% if tricycle == "TRICYCLE" %}
    DynEnumThree equals TRICYCLE string, as expected
    {% else %}
    DynEnumThree should equal TRICYCLE string, but does not
    {% endif %}

    {% if tricycle != "TRIANGLE" %}
    DynEnumThree is not equal to TRIANGLE string, as expected
    {% else %}
    DynEnumThree should not equal TRIANGLE string, but it does
    {% endif %}

    Multiple value tests:

    {% if tricycle == DynEnumThree.TRICYCLE or tricycle == DynEnumThree.TRIANGLE %}
    DynEnumThree is either TRICYCLE or TRIANGLE
    {% elif tricycle == DynEnumThree.TRIANGLE %}
    DynEnumThree is TRIANGLE
    {% else %}
    DynEnumThree is unknown
    {% endif %}

    {% if other == DynEnumThree.TRICYCLE %}
    Other is TRICYCLE
    {% else %}
    Other is not TRICYCLE
    {% endif %}

    {% if other == "TRIPOD" %}
    Other is TRIPOD
    {% else %}
    Other is not TRIPOD
    {% endif %}
  "#
}
