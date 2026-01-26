type ProjectId = int

function NormalAlias(pid: ProjectId) -> string {
  client "openai/gpt-4o"
  prompt #"Pid: {{ pid.id }}. Generate a fake name for it."#
}

type A = float
type B = A
type C = B

function LongerAlias(c: C) -> string {
  client "openai/gpt-4o"
  prompt #"{{ c.value }}"#
}

type JsonValue = int | string | bool | float | JsonObject | JsonArray
type JsonObject = map<string, JsonValue>
type JsonArray = JsonValue[]

function RecursiveAliases(j: JsonValue) -> string {
  client "openai/gpt-4o"
  prompt #"{{ j.value }}"#
}

type I = J
type J = I

function InvalidAlias(i: I) -> string {
  client "openai/gpt-4o"
  prompt #"{{ i.value }}"#
}

// warning: 'pid' is a type alias ProjectId (resolves to int), expected class
//   -->  class/type_aliases_jinja.baml:5
//    | 
//  4 |   client "openai/gpt-4o"
//  5 |   prompt #"Pid: {{ pid.id }}. Generate a fake name for it."#
//    | 
// warning: 'c' is a type alias C (resolves to float), expected class
//   -->  class/type_aliases_jinja.baml:14
//    | 
// 13 |   client "openai/gpt-4o"
// 14 |   prompt #"{{ c.value }}"#
//    | 
// warning: 'j' is a recursive type alias JsonValue, expected class
//   -->  class/type_aliases_jinja.baml:23
//    | 
// 22 |   client "openai/gpt-4o"
// 23 |   prompt #"{{ j.value }}"#
//    | 
// warning: 'i' is a recursive type alias I, expected class
//   -->  class/type_aliases_jinja.baml:31
//    | 
// 30 |   client "openai/gpt-4o"
// 31 |   prompt #"{{ i.value }}"#
//    | 
// error: Error validating: These aliases form a dependency cycle: I -> J
//   -->  class/type_aliases_jinja.baml:26
//    | 
// 25 | 
// 26 | type I = J
//    |

//----
//- diagnostics
// <no-diagnostics-expected>
