// Test: Nominal typing - same shape, different names should error

// Person and Human have identical fields
class Person {
	name string
	age int
}

class Human {
	name string
	age int
}

// Animal and Creature have identical fields
class Animal {
	species string
	legs int
}

class Creature {
	species string
	legs int
}

function NominalTypingAssignments() -> int {
	// === VALID ===
	let p1 = Person { name: "A", age: 1 };
	p1 = Person { name: "B", age: 2 };

	let h1 = Human { name: "C", age: 3 };
	h1 = Human { name: "D", age: 4 };

	let a1 = Animal { species: "Cat", legs: 4 };
	a1 = Animal { species: "Dog", legs: 4 };

	let c1 = Creature { species: "Bird", legs: 2 };
	c1 = Creature { species: "Fish", legs: 0 };

	// === INVALID (same shape but different nominal type) ===

	// Person <- Human
	let p2 = Person { name: "E", age: 5 };
	p2 = Human { name: "F", age: 6 };

	// Human <- Person
	let h2 = Human { name: "G", age: 7 };
	h2 = Person { name: "H", age: 8 };

	// Animal <- Creature
	let a2 = Animal { species: "Snake", legs: 0 };
	a2 = Creature { species: "Lizard", legs: 4 };

	// Creature <- Animal
	let c2 = Creature { species: "Ant", legs: 6 };
	c2 = Animal { species: "Spider", legs: 8 };

	// Person[] <- Human[]
	let people: Person[] = [Person { name: "I", age: 9 }];
	people = [Human { name: "J", age: 10 }];

	// Human[] <- Person[]
	let humans: Human[] = [Human { name: "K", age: 11 }];
	humans = [Person { name: "L", age: 12 }];

	1
}

//----
//- diagnostics
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_nominal_typing.baml:43:7]
//  43 │     p2 = Human { name: "F", age: 6 };
//     ·          ─────────────┬─────────────
//     ·                       ╰── expected `Person`, found `Human`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_nominal_typing.baml:47:7]
//  47 │     h2 = Person { name: "H", age: 8 };
//     ·          ──────────────┬─────────────
//     ·                        ╰── expected `Human`, found `Person`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_nominal_typing.baml:51:7]
//  51 │     a2 = Creature { species: "Lizard", legs: 4 };
//     ·          ───────────────────┬───────────────────
//     ·                             ╰── expected `Animal`, found `Creature`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_nominal_typing.baml:55:7]
//  55 │     c2 = Animal { species: "Spider", legs: 8 };
//     ·          ──────────────────┬──────────────────
//     ·                            ╰── expected `Creature`, found `Animal`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_nominal_typing.baml:59:12]
//  59 │     people = [Human { name: "J", age: 10 }];
//     ·               ──────────────┬─────────────
//     ·                             ╰── expected `Person`, found `Human`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_nominal_typing.baml:63:12]
//  63 │     humans = [Person { name: "L", age: 12 }];
//     ·               ──────────────┬──────────────
//     ·                             ╰── expected `Human`, found `Person`
//     ╰────
