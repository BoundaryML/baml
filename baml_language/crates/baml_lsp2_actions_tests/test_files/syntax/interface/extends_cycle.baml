// Direct cycle: A requires B; B requires A.

interface A requires B {}

interface B requires A {}

//----
//- diagnostics
// error: interface `requires` chain forms a cycle: A -> B -> A
//    ╭─[ interface_extends_cycle.baml:3:11 ]
//    │
//  3 │ interface A requires B {}
//    │           ┬  
//    │           ╰── cyclic `requires`
//    │ 
//    │ Note: Error code: E0118
// ───╯
// error: interface `requires` chain forms a cycle: B -> A -> B
//    ╭─[ interface_extends_cycle.baml:5:11 ]
//    │
//  5 │ interface B requires A {}
//    │           ┬  
//    │           ╰── cyclic `requires`
//    │ 
//    │ Note: Error code: E0118
// ───╯
