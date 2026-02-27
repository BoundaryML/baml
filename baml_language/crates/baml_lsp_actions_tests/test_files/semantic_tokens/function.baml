class Input {
    text string
}

function classify(input: Input) -> string {
    input.text
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// function.baml:1:1 (keyword) len=5 "class"
// function.baml:1:7 (class) len=5 "Input"
// function.baml:2:5 (property) len=4 "text"
// function.baml:2:10 (type) len=6 "string"
// function.baml:5:1 (keyword) len=8 "function"
// function.baml:5:10 (function) len=8 "classify"
// function.baml:5:19 (parameter) len=5 "input"
// function.baml:5:26 (class) len=5 "Input"
// function.baml:5:36 (type) len=6 "string"
// function.baml:6:5 (parameter) len=5 "input"
// function.baml:6:11 (property) len=4 "text"
