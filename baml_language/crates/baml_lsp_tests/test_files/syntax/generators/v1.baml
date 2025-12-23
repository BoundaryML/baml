generator lang_python_1 {
  output_type python/pydantic
  output_dir "../"
}

generator lang_python_2 {
  output_type python/pydantic
  default_client_mode "async"
  output_dir "../"
}

generator lang_python_3 {
  output_type python/pydantic
  default_client_mode "sync"
  output_dir "../"
}

generator lang_typescript_1 {
  output_type typescript
  default_client_mode "async"
  output_dir "../"
}


generator lang_typescript_2 {
  output_type typescript
  default_client_mode "sync"
  output_dir "../"
}

generator lang_ruby_1 {
  output_type typescript
  default_client_mode "async"
  output_dir "../"
}

generator lang_ruby_2 {
  output_type typescript
  default_client_mode "sync"
  output_dir "../"
}

//----
//- diagnostics
// Error: Expected top-level declaration, found Generator
//    ╭─[ 0:1:1 ]
//    │
//  1 │ generator lang_python_1 {
//    │ ────┬────  
//    │     ╰────── Expected top-level declaration, found Generator
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:1:11 ]
//    │
//  1 │ generator lang_python_1 {
//    │           ──────┬──────  
//    │                 ╰──────── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LBrace
//    ╭─[ 0:1:25 ]
//    │
//  1 │ generator lang_python_1 {
//    │                         ┬  
//    │                         ╰── Expected top-level declaration, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:2:3 ]
//    │
//  2 │   output_type python/pydantic
//    │   ─────┬─────  
//    │        ╰─────── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:2:15 ]
//    │
//  2 │   output_type python/pydantic
//    │               ───┬──  
//    │                  ╰──── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Slash
//    ╭─[ 0:2:21 ]
//    │
//  2 │   output_type python/pydantic
//    │                     ┬  
//    │                     ╰── Expected top-level declaration, found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:2:22 ]
//    │
//  2 │   output_type python/pydantic
//    │                      ────┬───  
//    │                          ╰───── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:3:3 ]
//    │
//  3 │   output_dir "../"
//    │   ─────┬────  
//    │        ╰────── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:3:14 ]
//    │
//  3 │   output_dir "../"
//    │              ┬  
//    │              ╰── Expected top-level declaration, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Dot
//    ╭─[ 0:3:15 ]
//    │
//  3 │   output_dir "../"
//    │               ┬  
//    │               ╰── Expected top-level declaration, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Dot
//    ╭─[ 0:3:16 ]
//    │
//  3 │   output_dir "../"
//    │                ┬  
//    │                ╰── Expected top-level declaration, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Slash
//    ╭─[ 0:3:17 ]
//    │
//  3 │   output_dir "../"
//    │                 ┬  
//    │                 ╰── Expected top-level declaration, found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:3:18 ]
//    │
//  3 │   output_dir "../"
//    │                  ┬  
//    │                  ╰── Expected top-level declaration, found Quote
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
// Error: Expected top-level declaration, found Generator
//    ╭─[ 0:6:1 ]
//    │
//  6 │ generator lang_python_2 {
//    │ ────┬────  
//    │     ╰────── Expected top-level declaration, found Generator
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:6:11 ]
//    │
//  6 │ generator lang_python_2 {
//    │           ──────┬──────  
//    │                 ╰──────── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LBrace
//    ╭─[ 0:6:25 ]
//    │
//  6 │ generator lang_python_2 {
//    │                         ┬  
//    │                         ╰── Expected top-level declaration, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:7:3 ]
//    │
//  7 │   output_type python/pydantic
//    │   ─────┬─────  
//    │        ╰─────── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:7:15 ]
//    │
//  7 │   output_type python/pydantic
//    │               ───┬──  
//    │                  ╰──── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Slash
//    ╭─[ 0:7:21 ]
//    │
//  7 │   output_type python/pydantic
//    │                     ┬  
//    │                     ╰── Expected top-level declaration, found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:7:22 ]
//    │
//  7 │   output_type python/pydantic
//    │                      ────┬───  
//    │                          ╰───── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:8:3 ]
//    │
//  8 │   default_client_mode "async"
//    │   ─────────┬─────────  
//    │            ╰─────────── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:8:23 ]
//    │
//  8 │   default_client_mode "async"
//    │                       ┬  
//    │                       ╰── Expected top-level declaration, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:8:24 ]
//    │
//  8 │   default_client_mode "async"
//    │                        ──┬──  
//    │                          ╰──── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:8:29 ]
//    │
//  8 │   default_client_mode "async"
//    │                             ┬  
//    │                             ╰── Expected top-level declaration, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:9:3 ]
//    │
//  9 │   output_dir "../"
//    │   ─────┬────  
//    │        ╰────── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:9:14 ]
//    │
//  9 │   output_dir "../"
//    │              ┬  
//    │              ╰── Expected top-level declaration, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Dot
//    ╭─[ 0:9:15 ]
//    │
//  9 │   output_dir "../"
//    │               ┬  
//    │               ╰── Expected top-level declaration, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Dot
//    ╭─[ 0:9:16 ]
//    │
//  9 │   output_dir "../"
//    │                ┬  
//    │                ╰── Expected top-level declaration, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Slash
//    ╭─[ 0:9:17 ]
//    │
//  9 │   output_dir "../"
//    │                 ┬  
//    │                 ╰── Expected top-level declaration, found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:9:18 ]
//    │
//  9 │   output_dir "../"
//    │                  ┬  
//    │                  ╰── Expected top-level declaration, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:10:1 ]
//     │
//  10 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Generator
//     ╭─[ 0:12:1 ]
//     │
//  12 │ generator lang_python_3 {
//     │ ────┬────  
//     │     ╰────── Expected top-level declaration, found Generator
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:12:11 ]
//     │
//  12 │ generator lang_python_3 {
//     │           ──────┬──────  
//     │                 ╰──────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:12:25 ]
//     │
//  12 │ generator lang_python_3 {
//     │                         ┬  
//     │                         ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:13:3 ]
//     │
//  13 │   output_type python/pydantic
//     │   ─────┬─────  
//     │        ╰─────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:13:15 ]
//     │
//  13 │   output_type python/pydantic
//     │               ───┬──  
//     │                  ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Slash
//     ╭─[ 0:13:21 ]
//     │
//  13 │   output_type python/pydantic
//     │                     ┬  
//     │                     ╰── Expected top-level declaration, found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:13:22 ]
//     │
//  13 │   output_type python/pydantic
//     │                      ────┬───  
//     │                          ╰───── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:14:3 ]
//     │
//  14 │   default_client_mode "sync"
//     │   ─────────┬─────────  
//     │            ╰─────────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:14:23 ]
//     │
//  14 │   default_client_mode "sync"
//     │                       ┬  
//     │                       ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:14:24 ]
//     │
//  14 │   default_client_mode "sync"
//     │                        ──┬─  
//     │                          ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:14:28 ]
//     │
//  14 │   default_client_mode "sync"
//     │                            ┬  
//     │                            ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:15:3 ]
//     │
//  15 │   output_dir "../"
//     │   ─────┬────  
//     │        ╰────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:15:14 ]
//     │
//  15 │   output_dir "../"
//     │              ┬  
//     │              ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:15:15 ]
//     │
//  15 │   output_dir "../"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:15:16 ]
//     │
//  15 │   output_dir "../"
//     │                ┬  
//     │                ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Slash
//     ╭─[ 0:15:17 ]
//     │
//  15 │   output_dir "../"
//     │                 ┬  
//     │                 ╰── Expected top-level declaration, found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:15:18 ]
//     │
//  15 │   output_dir "../"
//     │                  ┬  
//     │                  ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:16:1 ]
//     │
//  16 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Generator
//     ╭─[ 0:18:1 ]
//     │
//  18 │ generator lang_typescript_1 {
//     │ ────┬────  
//     │     ╰────── Expected top-level declaration, found Generator
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:18:11 ]
//     │
//  18 │ generator lang_typescript_1 {
//     │           ────────┬────────  
//     │                   ╰────────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:18:29 ]
//     │
//  18 │ generator lang_typescript_1 {
//     │                             ┬  
//     │                             ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:19:3 ]
//     │
//  19 │   output_type typescript
//     │   ─────┬─────  
//     │        ╰─────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:19:15 ]
//     │
//  19 │   output_type typescript
//     │               ─────┬────  
//     │                    ╰────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:20:3 ]
//     │
//  20 │   default_client_mode "async"
//     │   ─────────┬─────────  
//     │            ╰─────────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:20:23 ]
//     │
//  20 │   default_client_mode "async"
//     │                       ┬  
//     │                       ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:20:24 ]
//     │
//  20 │   default_client_mode "async"
//     │                        ──┬──  
//     │                          ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:20:29 ]
//     │
//  20 │   default_client_mode "async"
//     │                             ┬  
//     │                             ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:21:3 ]
//     │
//  21 │   output_dir "../"
//     │   ─────┬────  
//     │        ╰────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:21:14 ]
//     │
//  21 │   output_dir "../"
//     │              ┬  
//     │              ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:21:15 ]
//     │
//  21 │   output_dir "../"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:21:16 ]
//     │
//  21 │   output_dir "../"
//     │                ┬  
//     │                ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Slash
//     ╭─[ 0:21:17 ]
//     │
//  21 │   output_dir "../"
//     │                 ┬  
//     │                 ╰── Expected top-level declaration, found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:21:18 ]
//     │
//  21 │   output_dir "../"
//     │                  ┬  
//     │                  ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:22:1 ]
//     │
//  22 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Generator
//     ╭─[ 0:25:1 ]
//     │
//  25 │ generator lang_typescript_2 {
//     │ ────┬────  
//     │     ╰────── Expected top-level declaration, found Generator
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:25:11 ]
//     │
//  25 │ generator lang_typescript_2 {
//     │           ────────┬────────  
//     │                   ╰────────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:25:29 ]
//     │
//  25 │ generator lang_typescript_2 {
//     │                             ┬  
//     │                             ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:26:3 ]
//     │
//  26 │   output_type typescript
//     │   ─────┬─────  
//     │        ╰─────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:26:15 ]
//     │
//  26 │   output_type typescript
//     │               ─────┬────  
//     │                    ╰────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:27:3 ]
//     │
//  27 │   default_client_mode "sync"
//     │   ─────────┬─────────  
//     │            ╰─────────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:27:23 ]
//     │
//  27 │   default_client_mode "sync"
//     │                       ┬  
//     │                       ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:27:24 ]
//     │
//  27 │   default_client_mode "sync"
//     │                        ──┬─  
//     │                          ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:27:28 ]
//     │
//  27 │   default_client_mode "sync"
//     │                            ┬  
//     │                            ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:28:3 ]
//     │
//  28 │   output_dir "../"
//     │   ─────┬────  
//     │        ╰────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:28:14 ]
//     │
//  28 │   output_dir "../"
//     │              ┬  
//     │              ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:28:15 ]
//     │
//  28 │   output_dir "../"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:28:16 ]
//     │
//  28 │   output_dir "../"
//     │                ┬  
//     │                ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Slash
//     ╭─[ 0:28:17 ]
//     │
//  28 │   output_dir "../"
//     │                 ┬  
//     │                 ╰── Expected top-level declaration, found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:28:18 ]
//     │
//  28 │   output_dir "../"
//     │                  ┬  
//     │                  ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:29:1 ]
//     │
//  29 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Generator
//     ╭─[ 0:31:1 ]
//     │
//  31 │ generator lang_ruby_1 {
//     │ ────┬────  
//     │     ╰────── Expected top-level declaration, found Generator
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:31:11 ]
//     │
//  31 │ generator lang_ruby_1 {
//     │           ─────┬─────  
//     │                ╰─────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:31:23 ]
//     │
//  31 │ generator lang_ruby_1 {
//     │                       ┬  
//     │                       ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:32:3 ]
//     │
//  32 │   output_type typescript
//     │   ─────┬─────  
//     │        ╰─────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:32:15 ]
//     │
//  32 │   output_type typescript
//     │               ─────┬────  
//     │                    ╰────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:33:3 ]
//     │
//  33 │   default_client_mode "async"
//     │   ─────────┬─────────  
//     │            ╰─────────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:33:23 ]
//     │
//  33 │   default_client_mode "async"
//     │                       ┬  
//     │                       ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:33:24 ]
//     │
//  33 │   default_client_mode "async"
//     │                        ──┬──  
//     │                          ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:33:29 ]
//     │
//  33 │   default_client_mode "async"
//     │                             ┬  
//     │                             ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:34:3 ]
//     │
//  34 │   output_dir "../"
//     │   ─────┬────  
//     │        ╰────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:34:14 ]
//     │
//  34 │   output_dir "../"
//     │              ┬  
//     │              ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:34:15 ]
//     │
//  34 │   output_dir "../"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:34:16 ]
//     │
//  34 │   output_dir "../"
//     │                ┬  
//     │                ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Slash
//     ╭─[ 0:34:17 ]
//     │
//  34 │   output_dir "../"
//     │                 ┬  
//     │                 ╰── Expected top-level declaration, found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:34:18 ]
//     │
//  34 │   output_dir "../"
//     │                  ┬  
//     │                  ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:35:1 ]
//     │
//  35 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Generator
//     ╭─[ 0:37:1 ]
//     │
//  37 │ generator lang_ruby_2 {
//     │ ────┬────  
//     │     ╰────── Expected top-level declaration, found Generator
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:37:11 ]
//     │
//  37 │ generator lang_ruby_2 {
//     │           ─────┬─────  
//     │                ╰─────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:37:23 ]
//     │
//  37 │ generator lang_ruby_2 {
//     │                       ┬  
//     │                       ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:38:3 ]
//     │
//  38 │   output_type typescript
//     │   ─────┬─────  
//     │        ╰─────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:38:15 ]
//     │
//  38 │   output_type typescript
//     │               ─────┬────  
//     │                    ╰────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:39:3 ]
//     │
//  39 │   default_client_mode "sync"
//     │   ─────────┬─────────  
//     │            ╰─────────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:39:23 ]
//     │
//  39 │   default_client_mode "sync"
//     │                       ┬  
//     │                       ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:39:24 ]
//     │
//  39 │   default_client_mode "sync"
//     │                        ──┬─  
//     │                          ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:39:28 ]
//     │
//  39 │   default_client_mode "sync"
//     │                            ┬  
//     │                            ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:40:3 ]
//     │
//  40 │   output_dir "../"
//     │   ─────┬────  
//     │        ╰────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:40:14 ]
//     │
//  40 │   output_dir "../"
//     │              ┬  
//     │              ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:40:15 ]
//     │
//  40 │   output_dir "../"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:40:16 ]
//     │
//  40 │   output_dir "../"
//     │                ┬  
//     │                ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Slash
//     ╭─[ 0:40:17 ]
//     │
//  40 │   output_dir "../"
//     │                 ┬  
//     │                 ╰── Expected top-level declaration, found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:40:18 ]
//     │
//  40 │   output_dir "../"
//     │                  ┬  
//     │                  ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:41:1 ]
//     │
//  41 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
