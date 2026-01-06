function DeclareMutableInFunction(x: int) -> int {

	let y = 3;

	y = 5;

	y
}

function MutableInArg(x: int) -> int {
	x = 3;
	x
}

function ErrorWhenAssigningToConst(x: int) -> int {
	x = 42;
	x
}

//----
//- diagnostics
// Error: Expected `3`, found `5`
//    ╭─[ mut_variables.baml:5:6 ]
//    │
//  5 │     y = 5;
//    │         ┬  
//    │         ╰── Expected `3`, found `5`
//    │ 
//    │ Note: Error code: E0001
// ───╯
