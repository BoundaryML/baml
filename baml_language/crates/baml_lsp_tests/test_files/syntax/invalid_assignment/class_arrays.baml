// Test: Array of class type assignment mismatches

class Person {
	name string
	age int
}

class Animal {
	species string
	legs int
}

function ClassArrayAssignments() -> int {
	// === VALID ===
	let people: Person[] = [Person { name: "A", age: 1 }];
	people = [Person { name: "B", age: 2 }];

	let animals: Animal[] = [Animal { species: "Cat", legs: 4 }];
	animals = [Animal { species: "Dog", legs: 4 }];

	// === INVALID ===

	// Person[] <- Animal[]
	let p1: Person[] = [Person { name: "C", age: 3 }];
	p1 = [Animal { species: "Bird", legs: 2 }];

	// Animal[] <- Person[]
	let a1: Animal[] = [Animal { species: "Fish", legs: 0 }];
	a1 = [Person { name: "D", age: 4 }];

	// Person[] <- Person (array vs single)
	let p2: Person[] = [Person { name: "E", age: 5 }];
	p2 = Person { name: "F", age: 6 };

	// Person <- Person[] (single vs array)
	let p3 = Person { name: "G", age: 7 };
	p3 = [Person { name: "H", age: 8 }];

	// int[] <- Person[]
	let nums = [1, 2, 3];
	nums = [Person { name: "I", age: 9 }];

	1
}

//----
//- diagnostics
// Error: Expected Person[], found Animal[]
//     ╭─[ invalid_assignment_class_arrays.baml:25:6 ]
//     │
//  25 │     p1 = [Animal { species: "Bird", legs: 2 }];
//     │         ───────────────────┬──────────────────  
//     │                            ╰──────────────────── Expected Person[], found Animal[]
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected Animal[], found Person[]
//     ╭─[ invalid_assignment_class_arrays.baml:29:6 ]
//     │
//  29 │     a1 = [Person { name: "D", age: 4 }];
//     │         ───────────────┬───────────────  
//     │                        ╰───────────────── Expected Animal[], found Person[]
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected Person[], found Person
//     ╭─[ invalid_assignment_class_arrays.baml:33:7 ]
//     │
//  33 │     p2 = Person { name: "F", age: 6 };
//     │          ──────────────┬─────────────  
//     │                        ╰─────────────── Expected Person[], found Person
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected Person, found Person[]
//     ╭─[ invalid_assignment_class_arrays.baml:37:6 ]
//     │
//  37 │     p3 = [Person { name: "H", age: 8 }];
//     │         ───────────────┬───────────────  
//     │                        ╰───────────────── Expected Person, found Person[]
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected int[], found Person[]
//     ╭─[ invalid_assignment_class_arrays.baml:41:8 ]
//     │
//  41 │     nums = [Person { name: "I", age: 9 }];
//     │           ───────────────┬───────────────  
//     │                          ╰───────────────── Expected int[], found Person[]
//     │ 
//     │ Note: Error code: E0001
// ────╯
