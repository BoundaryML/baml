// Tests for validating map types

client<llm> MyClient {
  provider openai
}

class MapDummy {
  inner string
}

class MapFields {
  a1 map<string, string>
  a2 map<string, int>
  a3 map<string, MapDummy>

  b1 map<int, string>
  b2 map<float, string>
  b3 map<MapDummy, string>
  b4 map<string?, string>
  b5 map<string | int, string>

  c1 string | map<string, string>
  c2 string | map<int, string>
  c3 string | map<string?, string>

  d1 map<>
  d2 map<string>
  d3 map<string, string, string>
}

function InputAndOutput(i1: map<string, string>, i2: map<MapDummy, string>) -> map<string, int> {
  client MyClient
  prompt #"
    Merge these: {{i1}} {{i2}}
  "#
}

// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/map_types2.baml:26
//    |
// 25 |
// 26 |   d1 map<>
// 27 |   d2 map<string>
//    |
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/map_types2.baml:27
//    |
// 26 |   d1 map<>
// 27 |   d2 map<string>
// 28 |   d3 map<string, string, string>
//    |
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/map_types2.baml:28
//    |
// 27 |   d2 map<string>
// 28 |   d3 map<string, string, string>
// 29 | }
//    |

//----
//- diagnostics
// Error: Expected type, found '>'
//     ╭─[ class_map_types2.baml:26:10 ]
//     │
//  26 │   d1 map<>
//     │          ┬  
//     │          ╰── Expected type, found '>'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: map<K, V> requires exactly 2 type parameters, found 0
//     ╭─[ class_map_types2.baml:26:5 ]
//     │
//  26 │   d1 map<>
//     │     ───┬──  
//     │        ╰──── map<K, V> requires exactly 2 type parameters, found 0
//     │ 
//     │ Note: Error code: E0039
// ────╯
// Error: map<K, V> requires exactly 2 type parameters, found 1
//     ╭─[ class_map_types2.baml:27:5 ]
//     │
//  27 │   d2 map<string>
//     │     ──────┬─────  
//     │           ╰─────── map<K, V> requires exactly 2 type parameters, found 1
//     │ 
//     │ Note: Error code: E0039
// ────╯
// Error: map<K, V> requires exactly 2 type parameters, found 3
//     ╭─[ class_map_types2.baml:28:5 ]
//     │
//  28 │   d3 map<string, string, string>
//     │     ──────────────┬─────────────  
//     │                   ╰─────────────── map<K, V> requires exactly 2 type parameters, found 3
//     │ 
//     │ Note: Error code: E0039
// ────╯
