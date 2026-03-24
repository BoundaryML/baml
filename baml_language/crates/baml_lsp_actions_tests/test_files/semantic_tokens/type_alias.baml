type Name = string

class User {
    name Name
}

class Admin {
    name Name
}

type Person = User | Admin;

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
//
//- semantic_tokens
// type_alias.baml:1:1 (keyword) len=4 "type"
// type_alias.baml:1:6 (type) len=4 "Name"
// type_alias.baml:1:11 (operator) len=1 "="
// type_alias.baml:1:13 (type) len=6 "string"
// type_alias.baml:3:1 (keyword) len=5 "class"
// type_alias.baml:3:7 (class) len=4 "User"
// type_alias.baml:4:5 (property) len=4 "name"
// type_alias.baml:4:10 (namespace) len=4 "Name"
// type_alias.baml:7:1 (keyword) len=5 "class"
// type_alias.baml:7:7 (class) len=5 "Admin"
// type_alias.baml:8:5 (property) len=4 "name"
// type_alias.baml:8:10 (namespace) len=4 "Name"
// type_alias.baml:11:1 (keyword) len=4 "type"
// type_alias.baml:11:6 (type) len=6 "Person"
// type_alias.baml:11:13 (operator) len=1 "="
// type_alias.baml:11:15 (namespace) len=4 "User"
// type_alias.baml:11:20 (operator) len=1 "|"
// type_alias.baml:11:22 (namespace) len=5 "Admin"
