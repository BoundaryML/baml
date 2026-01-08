class Output {
    result string
}

function GetOutput<[CURSOR]() -> Output {
    Output { result: "hello" }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at hover_function.baml:5:19
// function GetOutput() -> Output
