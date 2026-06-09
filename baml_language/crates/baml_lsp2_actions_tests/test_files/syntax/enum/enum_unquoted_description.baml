enum TestEnum {
  Angry @alias("k1") @description(#"
    User is angry
  "#)
  Happy @alias("k2") @description(#"
    User is happy
  "#)
  // tests whether k1 doesnt incorrectly get matched with k11
  Sad @alias("k3") @description(#"
    User is sad
  "#)
  Confused @description(
    User is confused
  )
  Excited @alias("k5") @description(
    User is excited
  )
  Exclamation @alias("k6") // only alias

  Bored @alias("k7") @description(#"
    User is bored
    With a long description
  "#)

  @@alias("Category")
}

//----
//- diagnostics
// error: Expected ')', found is
//     ╭─[ enum_enum_unquoted_description.baml:13:10 ]
//     │
//  13 │     User is confused
//     │          ─┬  
//     │           ╰── Expected ')', found is
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected Unexpected token in enum body, found is
//     ╭─[ enum_enum_unquoted_description.baml:13:10 ]
//     │
//  13 │     User is confused
//     │          ─┬  
//     │           ╰── Expected Unexpected token in enum body, found is
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected Unexpected token in enum body, found ')'
//     ╭─[ enum_enum_unquoted_description.baml:14:3 ]
//     │
//  14 │   )
//     │   ┬  
//     │   ╰── Expected Unexpected token in enum body, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected ')', found is
//     ╭─[ enum_enum_unquoted_description.baml:16:10 ]
//     │
//  16 │     User is excited
//     │          ─┬  
//     │           ╰── Expected ')', found is
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected Unexpected token in enum body, found is
//     ╭─[ enum_enum_unquoted_description.baml:16:10 ]
//     │
//  16 │     User is excited
//     │          ─┬  
//     │           ╰── Expected Unexpected token in enum body, found is
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected Unexpected token in enum body, found ')'
//     ╭─[ enum_enum_unquoted_description.baml:17:3 ]
//     │
//  17 │   )
//     │   ┬  
//     │   ╰── Expected Unexpected token in enum body, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: `@description` argument must be a string literal, got `User`
//     ╭─[ enum_enum_unquoted_description.baml:12:25 ]
//     │
//  12 │ ╭─▶   Confused @description(
//  13 │ ├─▶     User is confused
//     │ │                          
//     │ ╰────────────────────────── invalid builtin declaration
//     │     
//     │     Note: Error code: E0037
// ────╯
// error: `@description` argument must be a string literal, got `User`
//     ╭─[ enum_enum_unquoted_description.baml:15:37 ]
//     │
//  15 │ ╭─▶   Excited @alias("k5") @description(
//  16 │ ├─▶     User is excited
//     │ │                         
//     │ ╰───────────────────────── invalid builtin declaration
//     │     
//     │     Note: Error code: E0037
// ────╯
