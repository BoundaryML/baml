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

// warning: Variable 'b' is not defined
//   -->  functions_v2/prompt_errors/prompt1.baml:6
//    | 
//  5 | template_string Foo() #"
//  6 |   This! {{ b}}
//    | 
// warning: Function 'Foo' expects 0 arguments, but got 1
//   -->  functions_v2/prompt_errors/prompt1.baml:24
//    | 
// 23 |   prompt #"
// 24 |     {{ Foo(a) }}
//    | 
// warning: 'b' is a (function Foo | function Foo2), expected function
//   -->  functions_v2/prompt_errors/prompt1.baml:37
//    | 
// 36 | 
// 37 |     {{ b() }}
//    | 
