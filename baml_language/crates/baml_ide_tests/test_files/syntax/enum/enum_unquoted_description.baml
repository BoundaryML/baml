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
// Error: Expected ')', found identifier
//     ╭─[ enum_enum_unquoted_description.baml:13:10 ]
//     │
//  13 │     User is confused
//     │          ─┬  
//     │           ╰── Expected ')', found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Unexpected token in enum body, found ')'
//     ╭─[ enum_enum_unquoted_description.baml:14:3 ]
//     │
//  14 │   )
//     │   ┬  
//     │   ╰── Expected Unexpected token in enum body, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected ')', found identifier
//     ╭─[ enum_enum_unquoted_description.baml:16:10 ]
//     │
//  16 │     User is excited
//     │          ─┬  
//     │           ╰── Expected ')', found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Unexpected token in enum body, found ')'
//     ╭─[ enum_enum_unquoted_description.baml:17:3 ]
//     │
//  17 │   )
//     │   ┬  
//     │   ╰── Expected Unexpected token in enum body, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Duplicate variant `is` in enum `TestEnum`
//     ╭─[ enum_enum_unquoted_description.baml:16:10 ]
//     │
//  16 │     User is excited
//     │          ─┬  
//     │           ╰── duplicate definition
//     │
//     ├─[ enum_enum_unquoted_description.baml:16:10 ]
//     │
//  13 │     User is confused
//     │          ─┬  
//     │           ╰── first definition here
//     │ 
//     │ Note: Error code: E0013
// ────╯
