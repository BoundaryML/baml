// throws never with catch on non-throwing callee — rethrow in unreachable arm is dead code

enum Errors {
  AuthenticationError
}

function getProfile(req: int) -> bool {
  return true
}

function server(req: int) -> bool throws never {
  getProfile(req) catch (e) {
    Errors.AuthenticationError => { throw e }
  }
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
// Error: unreachable arm
//     ╭─[ throw_throws_never_unreachable_rethrow.baml:13:36 ]
//     │
//  13 │ ╭─▶     Errors.AuthenticationError => { throw e }
//  14 │ ├─▶   }
//     │ │         
//     │ ╰───────── unreachable arm
//     │     
//     │     Note: Error code: E0001
// ────╯
