client<llm> Bar {
  provider baml-openai-chat
}

template_string Foo() #"
  This! {{ b}}
"#

template_string Foo2() #"
  Also This!
"#

function FooBar(a: string, b: int | bool) -> int {
  client Bar
  prompt #"
    {{ Foo() }}
  "#
}


function FooBar1(a: string, b: int | bool) -> int {
  client Bar
  prompt #"
    {{ Foo(a) }}
  "#
}

function Bar1(a: string) -> int {
  client Bar
  prompt #"
    {% if a == "a" %}
      {% set b = Foo %}
    {% else %}
      {% set b = Foo2 %}
    {% endif %}

    {{ b() }}
  "#
}

// error: Error validating function "Bar": Invalid function syntax. Valid function syntax is
// ```
// function Bar(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  functions_v2/prompt_errors/prompt1.baml:1
//    | 
//    | 
//  1 | client<llm> Bar {
//  2 |   provider baml-openai-chat
//  3 | }
//    | 
