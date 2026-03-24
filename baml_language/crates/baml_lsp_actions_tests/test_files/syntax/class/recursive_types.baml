// Trivial recursive type.
class Node {
  data int
  next Node?
}

// Mutually recursive types.
class Tree {
  data int
  children Forest
}

class Forest {
  trees Tree[]
}

// Unions.
class BasicUnion {
  data int | BasicUnion
}

class OptionalUnion {
  data OptionalUnion? | OptionalUnion
}

class FullyOptionalUnion {
  data (FullyOptionalUnion | FullyOptionalUnion)?
}

class NestedUnion {
  data NestedUnion | BasicUnion
}

class UnionOfUnions {
  data (Node | (BasicUnion | Tree)) | (NestedUnion | UnionOfUnions) | Forest
}

class RecursiveMap {
  data map<string, RecursiveMap>
}

//----
//- diagnostics
// Error: unreachable arm
//      ╭─[ llm_types.baml:159:17 ]
//      │
//  159 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:189:17 ]
//      │
//  189 │ ╭─▶             _ => {
//      ┆ ┆   
//  194 │ ├─▶             }
//      │ │                   
//      │ ╰─────────────────── unreachable arm
//      │     
//      │     Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:208:17 ]
//      │
//  208 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
