function ForLoopWithHeaders() -> int {
    //# Loop Processing
    let items = [1, 2, 3, 4, 5];
    let result = 0;

    //## Main Loop
    for (let item in items) {
        //### Item Processing
        let processed = item * 2;

        //#### Accumulation
        let result2 = result + processed;

        result2
    }

    //## Final Result
    result
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  headers/mdx_headers_for_loops.baml:1
//    |
//    |
//  1 | function ForLoopWithHeaders() -> int {
//    |

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// header_comments.baml:1:1 (keyword) len=8 "function"
// header_comments.baml:1:10 (function) [declaration] len=18 "ForLoopWithHeaders"
// header_comments.baml:1:34 (type) [defaultLibrary] len=3 "int"
// header_comments.baml:2:5 (comment) len=1 "/"
// header_comments.baml:2:6 (comment) len=1 "/"
// header_comments.baml:2:7 (comment) len=1 "#"
// header_comments.baml:2:9 (comment) len=4 "Loop"
// header_comments.baml:2:14 (comment) len=10 "Processing"
// header_comments.baml:3:5 (keyword) len=3 "let"
// header_comments.baml:3:9 (variable) [declaration] len=5 "items"
// header_comments.baml:3:15 (operator) len=1 "="
// header_comments.baml:3:18 (number) len=1 "1"
// header_comments.baml:3:21 (number) len=1 "2"
// header_comments.baml:3:24 (number) len=1 "3"
// header_comments.baml:3:27 (number) len=1 "4"
// header_comments.baml:3:30 (number) len=1 "5"
// header_comments.baml:4:5 (keyword) len=3 "let"
// header_comments.baml:4:9 (variable) [declaration] len=6 "result"
// header_comments.baml:4:16 (operator) len=1 "="
// header_comments.baml:4:18 (number) len=1 "0"
// header_comments.baml:6:5 (comment) len=1 "/"
// header_comments.baml:6:6 (comment) len=1 "/"
// header_comments.baml:6:7 (comment) len=1 "#"
// header_comments.baml:6:8 (comment) len=1 "#"
// header_comments.baml:6:10 (comment) len=4 "Main"
// header_comments.baml:6:15 (comment) len=4 "Loop"
// header_comments.baml:7:5 (keyword) len=3 "for"
// header_comments.baml:7:10 (keyword) len=3 "let"
// header_comments.baml:7:14 (variable) [declaration] len=4 "item"
// header_comments.baml:7:19 (keyword) len=2 "in"
// header_comments.baml:7:22 (variable) len=5 "items"
// header_comments.baml:8:9 (comment) len=1 "/"
// header_comments.baml:8:10 (comment) len=1 "/"
// header_comments.baml:8:11 (comment) len=1 "#"
// header_comments.baml:8:12 (comment) len=1 "#"
// header_comments.baml:8:13 (comment) len=1 "#"
// header_comments.baml:8:15 (comment) len=4 "Item"
// header_comments.baml:8:20 (comment) len=10 "Processing"
// header_comments.baml:9:9 (keyword) len=3 "let"
// header_comments.baml:9:13 (variable) [declaration] len=9 "processed"
// header_comments.baml:9:23 (operator) len=1 "="
// header_comments.baml:9:25 (variable) len=4 "item"
// header_comments.baml:9:30 (operator) len=1 "*"
// header_comments.baml:9:32 (number) len=1 "2"
// header_comments.baml:11:9 (comment) len=1 "/"
// header_comments.baml:11:10 (comment) len=1 "/"
// header_comments.baml:11:11 (comment) len=1 "#"
// header_comments.baml:11:12 (comment) len=1 "#"
// header_comments.baml:11:13 (comment) len=1 "#"
// header_comments.baml:11:14 (comment) len=1 "#"
// header_comments.baml:11:16 (comment) len=12 "Accumulation"
// header_comments.baml:12:9 (keyword) len=3 "let"
// header_comments.baml:12:13 (variable) [declaration] len=7 "result2"
// header_comments.baml:12:21 (operator) len=1 "="
// header_comments.baml:12:23 (variable) len=6 "result"
// header_comments.baml:12:30 (operator) len=1 "+"
// header_comments.baml:12:32 (variable) len=9 "processed"
// header_comments.baml:14:9 (variable) len=7 "result2"
// header_comments.baml:17:5 (comment) len=1 "/"
// header_comments.baml:17:6 (comment) len=1 "/"
// header_comments.baml:17:7 (comment) len=1 "#"
// header_comments.baml:17:8 (comment) len=1 "#"
// header_comments.baml:17:10 (comment) len=5 "Final"
// header_comments.baml:17:16 (comment) len=6 "Result"
// header_comments.baml:18:5 (variable) len=6 "result"
// header_comments.baml:21:1 (comment) len=78 "// warning: Workflow functions are experimental, and will break in the future."
// header_comments.baml:22:1 (comment) len=46 "//   -->  headers/mdx_headers_for_loops.baml:1"
// header_comments.baml:23:1 (comment) len=7 "//    |"
// header_comments.baml:24:1 (comment) len=7 "//    |"
// header_comments.baml:25:1 (comment) len=46 "//  1 | function ForLoopWithHeaders() -> int {"
// header_comments.baml:26:1 (comment) len=7 "//    |"
