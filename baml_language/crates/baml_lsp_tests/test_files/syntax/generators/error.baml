generator default {
  language python
  o o
}

// error: Property not known: "language". Did you mean one of these: "version", "on_generate", "project", "output_type", "output_dir", "module_format", "client_package_name", "default_client_mode"?
//   -->  generators/error.baml:2
//    | 
//  1 | generator default {
//  2 |   language python
//    | 
// error: Property not known: "o". Did you mean one of these: "version", "project", "output_dir", "output_type", "on_generate", "module_format", "default_client_mode", "client_package_name"?
//   -->  generators/error.baml:3
//    | 
//  2 |   language python
//  3 |   o o
//    |

//----
//- diagnostics
// Error: Expected top-level declaration, found Generator
//    ╭─[ 0:1:1 ]
//    │
//  1 │ generator default {
//    │ ────┬────  
//    │     ╰────── Expected top-level declaration, found Generator
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:1:11 ]
//    │
//  1 │ generator default {
//    │           ───┬───  
//    │              ╰───── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LBrace
//    ╭─[ 0:1:19 ]
//    │
//  1 │ generator default {
//    │                   ┬  
//    │                   ╰── Expected top-level declaration, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:2:3 ]
//    │
//  2 │   language python
//    │   ────┬───  
//    │       ╰───── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:2:12 ]
//    │
//  2 │   language python
//    │            ───┬──  
//    │               ╰──── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:3:3 ]
//    │
//  3 │   o o
//    │   ┬  
//    │   ╰── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:3:5 ]
//    │
//  3 │   o o
//    │     ┬  
//    │     ╰── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RBrace
//    ╭─[ 0:4:1 ]
//    │
//  4 │ }
//    │ ┬  
//    │ ╰── Expected top-level declaration, found RBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
