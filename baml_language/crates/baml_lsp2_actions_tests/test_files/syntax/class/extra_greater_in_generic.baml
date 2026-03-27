// Test: Extra '>' in generic type should not cause cascading errors
// The pending_greaters counter is now reset at type boundaries to prevent
// errors in one field from cascading into subsequent fields.

class TestClass {
  bad_field map<string, int>>
  good_field map<string, int>
  another_field string
}

//----
//- diagnostics
// Error: Unmatched '>' in type expression (found 1 extra)
//    ╭─[ class_extra_greater_in_generic.baml:6:29 ]
//    │
//  6 │   bad_field map<string, int>>
//    │                             ┬  
//    │                             ╰── Unmatched '>' in type expression (found 1 extra)
//    │ 
//    │ Note: Error code: E0010
// ───╯
