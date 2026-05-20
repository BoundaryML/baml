client<llm> Bar {
  provider openai
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

// warning: Variable `b` does not exist. Did you mean one of these: `_`, `ctx`?
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
// warning: Function 'Foo' referenced without parentheses. Did you mean 'Foo()'?
//   -->  functions_v2/prompt_errors/prompt1.baml:32
//    |
// 31 |     {% if a == "a" %}
// 32 |       {% set b = Foo %}
//    |
// warning: Function 'Foo2' referenced without parentheses. Did you mean 'Foo2()'?
//   -->  functions_v2/prompt_errors/prompt1.baml:34
//    |
// 33 |     {% else %}
// 34 |       {% set b = Foo2 %}
//    |
// warning: 'b' is undefined, expected function
//   -->  functions_v2/prompt_errors/prompt1.baml:37
//    |
// 36 |
// 37 |     {{ b() }}
//    |
// warning: Variable `b` does not exist. Did you mean one of these: `_`, `ctx`?
//   -->  functions_v2/prompt_errors/prompt1.baml:6
//    |
//  5 | template_string Foo() #"
//  6 |   This! {{ b}}
//    |

//----
//- diagnostics
// warning: Function 'Foo' referenced without parentheses. Did you mean 'Foo()'?
//     ╭─[ functions_v2_prompt_errors_prompt1.baml:32:18 ]
//     │
//  32 │       {% set b = Foo %}
//     │                  ─┬─  
//     │                   ╰─── Function 'Foo' referenced without parentheses. Did you mean 'Foo()'?
//     │ 
//     │ Note: Error code: E0071
// ────╯
// warning: Function 'Foo2' referenced without parentheses. Did you mean 'Foo2()'?
//     ╭─[ functions_v2_prompt_errors_prompt1.baml:34:18 ]
//     │
//  34 │       {% set b = Foo2 %}
//     │                  ──┬─  
//     │                    ╰─── Function 'Foo2' referenced without parentheses. Did you mean 'Foo2()'?
//     │ 
//     │ Note: Error code: E0071
// ────╯
// warning: 'b' is undefined, expected function
//     ╭─[ functions_v2_prompt_errors_prompt1.baml:37:8 ]
//     │
//  37 │     {{ b() }}
//     │        ─┬─  
//     │         ╰─── 'b' is undefined, expected function
//     │ 
//     │ Note: Error code: E0073
// ────╯
// warning: Function 'Foo' expects 0 arguments, but got 1
//     ╭─[ functions_v2_prompt_errors_prompt1.baml:24:8 ]
//     │
//  24 │     {{ Foo(a) }}
//     │        ───┬──  
//     │           ╰──── Function 'Foo' expects 0 arguments, but got 1
//     │ 
//     │ Note: Error code: E0080
// ────╯
// warning: Variable `b` does not exist. Did you mean one of these: `_`, `baml`, `ctx`?
//    ╭─[ functions_v2_prompt_errors_prompt1.baml:6:12 ]
//    │
//  6 │   This! {{ b}}
//    │            ┬  
//    │            ╰── Variable `b` does not exist. Did you mean one of these: `_`, `baml`, `ctx`?
//    │ 
//    │ Note: Error code: E0070
// ───╯
