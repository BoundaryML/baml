class Foo {
  bar string @check(bar_check, {{ ) }})
}


function FunctionName(arg:string) -> "foo" {
  client "openai/gpt-4o"
  prompt #"
    Your prompt here in jinja format
    {{ ) }}
  "#
}


function FunctionName2(arg:string) -> "foo" {
  client "openai/gpt-4o"
  prompt #"
    Your prompt here in jinja format
    {{ if foo }}
    {{ foo }}
    {{ endif }}
  "#
}

// error: Error validating: Error parsing jinja template: syntax error: unexpected `)` (in <expression>:1)
//   -->  constraints/malformed_expression.baml:2
//    |
//  1 | class Foo {
//  2 |   bar string @check(bar_check, {{ ) }})
//    |
// error: Error validating: Error parsing jinja template: syntax error: unexpected `)` (in FunctionName:3)
//   -->  constraints/malformed_expression.baml:10
//    |
//  9 |     Your prompt here in jinja format
// 10 |     {{ ) }}
//    |
// error: Error validating: Error parsing jinja template: syntax error: unexpected identifier, expected end of variable block (in FunctionName2:3)
//   -->  constraints/malformed_expression.baml:19
//    |
// 18 |     Your prompt here in jinja format
// 19 |     {{ if foo }}
//    |

//----
//- diagnostics
// <no-diagnostics-expected>
