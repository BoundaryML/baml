// Test: Class type assignment mismatches

class Person {
	name string
	age int
}

class Animal {
	species string
	legs int
}

function ClassAssignments() -> int {
	// === VALID ===
	let p1 = Person { name: "Alice", age: 30 };
	p1 = Person { name: "Bob", age: 25 };

	let a1 = Animal { species: "Cat", legs: 4 };
	a1 = Animal { species: "Dog", legs: 4 };

	// === INVALID ===

	// Person <- Animal
	let p2 = Person { name: "Charlie", age: 35 };
	p2 = Animal { species: "Bird", legs: 2 };

	// Animal <- Person
	let a2 = Animal { species: "Fish", legs: 0 };
	a2 = Person { name: "Dave", age: 40 };

	// Person <- string
	let p3 = Person { name: "Eve", age: 28 };
	p3 = "not a person";

	// Person <- int
	let p4 = Person { name: "Frank", age: 45 };
	p4 = 42;

	// int <- Person
	let n = 100;
	n = Person { name: "Grace", age: 32 };

	1
}

//----
//- diagnostics
// Error: Expected Person, found Animal
//     ╭─[ 0:25:7 ]
//     │
//  25 │     p2 = Animal { species: "Bird", legs: 2 };
//     │          ─────────────────┬─────────────────  
//     │                           ╰─────────────────── Expected Person, found Animal
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected Animal, found Person
//     ╭─[ 0:29:7 ]
//     │
//  29 │     a2 = Person { name: "Dave", age: 40 };
//     │          ────────────────┬───────────────  
//     │                          ╰───────────────── Expected Animal, found Person
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected Person, found string
//     ╭─[ 0:33:6 ]
//     │
//  33 │     p3 = "not a person";
//     │         ───────┬───────  
//     │                ╰───────── Expected Person, found string
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected Person, found int
//     ╭─[ 0:37:7 ]
//     │
//  37 │     p4 = 42;
//     │          ─┬  
//     │           ╰── Expected Person, found int
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected int, found Person
//     ╭─[ 0:41:6 ]
//     │
//  41 │     n = Person { name: "Grace", age: 32 };
//     │         ────────────────┬────────────────  
//     │                         ╰────────────────── Expected int, found Person
//     │ 
//     │ Note: Error code: E0001
// ────╯
