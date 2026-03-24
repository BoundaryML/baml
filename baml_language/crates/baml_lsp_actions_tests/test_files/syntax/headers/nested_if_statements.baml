//# Main Function
function NestedIfStatements() -> string {
    //## If statement 1
    let e = if (true) {

        //### If statement 2.1
        let z = if (true) {
            //### If statement 3.1
            let a = if (true) {
                //### True
                12
            } else {
                //### Third False
                13
            };

            242
        } else {
            //### If statement 3.2
            let a = if (true) {
                //### True
                12
            } else {
                //### Third False
                13
            };
            13
        };

        2
    } else {
        //### If statement 2.2
        let z = if (true) {
            //### If statement 3.1
            let a = if (true) {
                //### True
                12
            } else {
                //### Third False
                13
            };

            242
        } else {
            //### If statement 3.2
            let a = if (true) {
                //### True
                12
            } else {
                //### Third False
                13
            };
            13
        };
        13
    };

    e
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
// Error: Duplicate binding `a` in `NestedIfStatements`
//     ╭─[ headers_nested_if_statements.baml:20:17 ]
//     │
//   9 │             let a = if (true) {
//     │                 ┬  
//     │                 ╰── first defined as binding here
//     │ 
//  20 │             let a = if (true) {
//     │                 ┬  
//     │                 ╰── duplicate binding definition
//     │ 
//  35 │             let a = if (true) {
//     │                 ┬  
//     │                 ╰── duplicate binding definition
//     │ 
//  46 │             let a = if (true) {
//     │                 ┬  
//     │                 ╰── duplicate binding definition
//     │ 
//     │ Note: Error code: E0012
// ────╯
// Error: Duplicate binding `z` in `NestedIfStatements`
//     ╭─[ headers_nested_if_statements.baml:33:13 ]
//     │
//   7 │         let z = if (true) {
//     │             ┬  
//     │             ╰── first defined as binding here
//     │ 
//  33 │         let z = if (true) {
//     │             ┬  
//     │             ╰── duplicate binding definition
//     │ 
//     │ Note: Error code: E0012
// ────╯
// Error: type mismatch: expected string, got 2 | 13
//     ╭─[ headers_nested_if_statements.baml:58:5 ]
//     │
//  58 │     e
//     │     ┬  
//     │     ╰── type mismatch: expected string, got 2 | 13
//     │ 
//     │ Note: Error code: E0001
// ────╯
