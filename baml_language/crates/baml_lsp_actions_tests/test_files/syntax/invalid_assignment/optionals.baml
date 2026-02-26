// Test: Optional type assignment mismatches

class Person {
	name string
	age int
}

class Animal {
	species string
	legs int
}

function OptionalAssignments() -> int {
	// === VALID ===
	let a: int? = 10;
	a = 20;
	a = null;

	let b: string? = "hello";
	b = "world";
	b = null;

	let p: Person? = Person { name: "A", age: 1 };
	p = Person { name: "B", age: 2 };
	p = null;

	// === INVALID ===

	// int? <- string
	let c: int? = 5;
	c = "not an int";

	// string? <- int
	let d: string? = "text";
	d = 42;

	// Person? <- Animal
	let e: Person? = Person { name: "C", age: 3 };
	e = Animal { species: "Cat", legs: 4 };

	1
}

//----
//- diagnostics
// Error: Expected `int?`, found `string`
//     ╭─[ invalid_assignment_optionals.baml:31:6 ]
//     │
//  31 │     c = "not an int";
//     │         ──────┬─────  
//     │               ╰─────── Expected `int?`, found `string`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `string?`, found `int`
//     ╭─[ invalid_assignment_optionals.baml:35:6 ]
//     │
//  35 │     d = 42;
//     │         ─┬  
//     │          ╰── Expected `string?`, found `int`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `Person?`, found `Animal`
//     ╭─[ invalid_assignment_optionals.baml:39:6 ]
//     │
//  39 │     e = Animal { species: "Cat", legs: 4 };
//     │         ─────────────────┬────────────────  
//     │                          ╰────────────────── Expected `Person?`, found `Animal`
//     │ 
//     │ Note: Error code: E0001
// ────╯
