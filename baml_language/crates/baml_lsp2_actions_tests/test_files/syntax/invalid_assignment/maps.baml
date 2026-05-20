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
// error: type mismatch: expected map<string, int>, got map<string, string>
//     ╭─[ invalid_assignment_maps.baml:28:5 ]
//     │
//  28 │     d = {"y": "wrong"};
//     │        ───────┬───────  
//     │               ╰───────── type mismatch: expected map<string, int>, got map<string, string>
//     │ 
//     │ Note: Error code: E0001
// ────╯
// error: type mismatch: expected map<string, user.Person>, got map<string, user.Animal>
//     ╭─[ invalid_assignment_maps.baml:32:5 ]
//     │
//  32 │     e = {"a": Animal { species: "Cat", legs: 4 }};
//     │        ─────────────────────┬────────────────────  
//     │                             ╰────────────────────── type mismatch: expected map<string, user.Person>, got map<string, user.Animal>
//     │ 
//     │ Note: Error code: E0001
// ────╯
// error: type mismatch: expected map<string, int>, got "not a map"
//     ╭─[ invalid_assignment_maps.baml:36:6 ]
//     │
//  36 │     f = "not a map";
//     │         ─────┬─────  
//     │              ╰─────── type mismatch: expected map<string, int>, got "not a map"
//     │ 
//     │ Note: Error code: E0001
// ────╯
// error: type mismatch: expected map<string, int>, got 123
//     ╭─[ invalid_assignment_maps.baml:40:6 ]
//     │
//  40 │     g = 123;
//     │         ─┬─  
//     │          ╰─── type mismatch: expected map<string, int>, got 123
//     │ 
//     │ Note: Error code: E0001
// ────╯
