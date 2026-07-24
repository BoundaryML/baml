// Test: Map type assignment mismatches

class Person {
	name string
	age int
}

class Animal {
	species string
	legs int
}

function MapAssignments() -> int {
	// === VALID ===
	let a: map<string, int> = {"x": 1};
	a = {"y": 2};

	let b: map<string, string> = {"a": "b"};
	b = {"c": "d"};

	let c: map<string, Person> = {"p": Person { name: "A", age: 1 }};
	c = {"q": Person { name: "B", age: 2 }};

	// === INVALID ===

	// map<string, int> <- map<string, string>
	let d: map<string, int> = {"x": 1};
	d = {"y": "wrong"};

	// map<string, Person> <- map<string, Animal>
	let e: map<string, Person> = {"p": Person { name: "C", age: 3 }};
	e = {"a": Animal { species: "Cat", legs: 4 }};

	// map<string, int> <- string
	let f: map<string, int> = {"z": 5};
	f = "not a map";

	// map<string, int> <- int
	let g: map<string, int> = {"w": 6};
	g = 123;

	// string <- map<string, int>
	let h = "text";
	h = {"k": 7};

	1
}

//----
//- diagnostics
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_maps.baml:28:12]
//  28 │     d = {"y": "wrong"};
//     ·               ───┬───
//     ·                  ╰── expected `int`, found `"wrong"`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_maps.baml:32:12]
//  32 │     e = {"a": Animal { species: "Cat", legs: 4 }};
//     ·               ─────────────────┬────────────────
//     ·                                ╰── expected `Person`, found `Animal`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_maps.baml:36:6]
//  36 │     f = "not a map";
//     ·         ─────┬─────
//     ·              ╰── expected `map<string, int>`, found `"not a map"`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_maps.baml:40:6]
//  40 │     g = 123;
//     ·         ─┬─
//     ·          ╰── expected `map<string, int>`, found `123`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_maps.baml:44:6]
//  44 │     h = {"k": 7};
//     ·         ────┬───
//     ·             ╰── expected `string`, found `map<string, int>`
//     ╰────
