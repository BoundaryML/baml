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
// Error: Type `any` is not allowed in catch bindings
//    ╭─[ catch_unknown_binding_types.baml:6:21 ]
//    │
//  6 │   MayFail(x) catch (e: any) {
//    │                     ───┬──  
//    │                        ╰──── Type `any` is not allowed in catch bindings
//    │ 
//    │ Note: Error code: E0093
// ───╯
// Error: Unknown type `any`
//    ╭─[ catch_unknown_binding_types.baml:6:21 ]
//    │
//  6 │   MayFail(x) catch (e: any) {
//    │                     ───┬──  
//    │                        ╰──── Unknown type `any`
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Warning: Unreachable catch arm
//    ╭─[ catch_unknown_binding_types.baml:7:5 ]
//    │
//  7 │     _ => "caught any"
//    │     ┬  
//    │     ╰── Unreachable catch arm
//    │ 
//    │ Note: Error code: E0095
// ───╯
// Error: Type `unknown` is not allowed in catch bindings
//    ╭─[ catch_unknown_binding_types.baml:8:12 ]
//    │
//  8 │   } catch (other: unknown) {
//    │            ───────┬──────  
//    │                   ╰──────── Type `unknown` is not allowed in catch bindings
//    │ 
//    │ Note: Error code: E0093
// ───╯
// Warning: Unreachable catch arm
//    ╭─[ catch_unknown_binding_types.baml:9:5 ]
//    │
//  9 │     _ => "caught unknown"
//    │     ┬  
//    │     ╰── Unreachable catch arm
//    │ 
//    │ Note: Error code: E0095
// ───╯
