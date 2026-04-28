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
// Error: Expected ')', found ':'
//    ╭─[ catch_unknown_binding_types.baml:6:22 ]
//    │
//  6 │   MayFail(x) catch (e: any) {
//    │                      ┬  
//    │                      ╰── Expected ')', found ':'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected catch clause body, found ':'
//    ╭─[ catch_unknown_binding_types.baml:6:22 ]
//    │
//  6 │   MayFail(x) catch (e: any) {
//    │                      ┬  
//    │                      ╰── Expected catch clause body, found ':'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found ':'
//    ╭─[ catch_unknown_binding_types.baml:6:22 ]
//    │
//  6 │   MayFail(x) catch (e: any) {
//    │                      ┬  
//    │                      ╰── Expected expression, found ':'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found ')'
//    ╭─[ catch_unknown_binding_types.baml:6:27 ]
//    │
//  6 │   MayFail(x) catch (e: any) {
//    │                           ┬  
//    │                           ╰── Expected expression, found ')'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found '=>'
//    ╭─[ catch_unknown_binding_types.baml:7:7 ]
//    │
//  7 │     _ => "caught any"
//    │       ─┬  
//    │        ╰── Expected expression, found '=>'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected ')', found ':'
//    ╭─[ catch_unknown_binding_types.baml:8:17 ]
//    │
//  8 │   } catch (other: unknown) {
//    │                 ┬  
//    │                 ╰── Expected ')', found ':'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected catch clause body, found ':'
//    ╭─[ catch_unknown_binding_types.baml:8:17 ]
//    │
//  8 │   } catch (other: unknown) {
//    │                 ┬  
//    │                 ╰── Expected catch clause body, found ':'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found ':'
//    ╭─[ catch_unknown_binding_types.baml:8:17 ]
//    │
//  8 │   } catch (other: unknown) {
//    │                 ┬  
//    │                 ╰── Expected expression, found ':'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found ')'
//    ╭─[ catch_unknown_binding_types.baml:8:26 ]
//    │
//  8 │   } catch (other: unknown) {
//    │                          ┬  
//    │                          ╰── Expected expression, found ')'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found '=>'
//    ╭─[ catch_unknown_binding_types.baml:9:7 ]
//    │
//  9 │     _ => "caught unknown"
//    │       ─┬  
//    │        ╰── Expected expression, found '=>'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: unresolved name: any
//    ╭─[ catch_unknown_binding_types.baml:6:24 ]
//    │
//  6 │   MayFail(x) catch (e: any) {
//    │                        ─┬─  
//    │                         ╰─── unresolved name: any
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: unresolved name: _
//    ╭─[ catch_unknown_binding_types.baml:7:5 ]
//    │
//  7 │     _ => "caught any"
//    │     ┬  
//    │     ╰── unresolved name: _
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: unresolved name: unknown
//    ╭─[ catch_unknown_binding_types.baml:8:19 ]
//    │
//  8 │   } catch (other: unknown) {
//    │                   ───┬───  
//    │                      ╰───── unresolved name: unknown
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: unresolved name: _
//    ╭─[ catch_unknown_binding_types.baml:9:5 ]
//    │
//  9 │     _ => "caught unknown"
//    │     ┬  
//    │     ╰── unresolved name: _
//    │ 
//    │ Note: Error code: E0003
// ───╯
