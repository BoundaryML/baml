function MayFail(x: int) -> string {
  "ok"
}

function UnknownBindingTypes(x: int) -> string {
  MayFail(x) catch (e: any) {
    _ => "caught any"
  } catch (other: unknown) {
    _ => "caught unknown"
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
// Error: invalid catch binding type `any`; use a concrete type instead
//    ╭─[ catch_unknown_binding_types.baml:6:3 ]
//    │
//  6 │   MayFail(x) catch (e: any) {
//    │   ─────┬────  
//    │        ╰────── invalid catch binding type `any`; use a concrete type instead
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: invalid catch binding type `unknown`; use a concrete type instead
//    ╭─[ catch_unknown_binding_types.baml:6:3 ]
//    │
//  6 │   MayFail(x) catch (e: any) {
//    │   ─────┬────  
//    │        ╰────── invalid catch binding type `unknown`; use a concrete type instead
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: unreachable arm
//    ╭─[ catch_unknown_binding_types.baml:7:10 ]
//    │
//  7 │     _ => "caught any"
//    │          ──────┬─────  
//    │                ╰─────── unreachable arm
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: unreachable arm
//    ╭─[ catch_unknown_binding_types.baml:9:10 ]
//    │
//  9 │     _ => "caught unknown"
//    │          ────────┬───────  
//    │                  ╰───────── unreachable arm
//    │ 
//    │ Note: Error code: E0001
// ───╯
