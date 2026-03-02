class Dog {
  type "dog"
  name string
}

class Cat {
  type "cat"
  name string
}

class Lizard {
  type "lizard"
  name string
}

type Pet = Cat | Dog | Lizard;

function Describe(pet: Pet) -> string {
  match (pet) {
    cat: Cat => cat.name,
    dog: Dog => dog.name,
    _ => "other pet",
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// match_expr.baml:1:1 (keyword) len=5 "class"
// match_expr.baml:1:7 (class) len=3 "Dog"
// match_expr.baml:2:3 (property) len=4 "type"
// match_expr.baml:2:8 (string) len=1 "\""
// match_expr.baml:2:9 (string) len=3 "dog"
// match_expr.baml:2:12 (string) len=1 "\""
// match_expr.baml:3:3 (property) len=4 "name"
// match_expr.baml:3:8 (type) len=6 "string"
// match_expr.baml:6:1 (keyword) len=5 "class"
// match_expr.baml:6:7 (class) len=3 "Cat"
// match_expr.baml:7:3 (property) len=4 "type"
// match_expr.baml:7:8 (string) len=1 "\""
// match_expr.baml:7:9 (string) len=3 "cat"
// match_expr.baml:7:12 (string) len=1 "\""
// match_expr.baml:8:3 (property) len=4 "name"
// match_expr.baml:8:8 (type) len=6 "string"
// match_expr.baml:11:1 (keyword) len=5 "class"
// match_expr.baml:11:7 (class) len=6 "Lizard"
// match_expr.baml:12:3 (property) len=4 "type"
// match_expr.baml:12:8 (string) len=1 "\""
// match_expr.baml:12:9 (string) len=6 "lizard"
// match_expr.baml:12:15 (string) len=1 "\""
// match_expr.baml:13:3 (property) len=4 "name"
// match_expr.baml:13:8 (type) len=6 "string"
// match_expr.baml:16:1 (keyword) len=4 "type"
// match_expr.baml:16:6 (type) len=3 "Pet"
// match_expr.baml:16:10 (operator) len=1 "="
// match_expr.baml:16:12 (class) len=3 "Cat"
// match_expr.baml:16:16 (operator) len=1 "|"
// match_expr.baml:16:18 (class) len=3 "Dog"
// match_expr.baml:16:22 (operator) len=1 "|"
// match_expr.baml:16:24 (class) len=6 "Lizard"
// match_expr.baml:18:1 (keyword) len=8 "function"
// match_expr.baml:18:10 (function) len=8 "Describe"
// match_expr.baml:18:19 (parameter) len=3 "pet"
// match_expr.baml:18:24 (type) len=3 "Pet"
// match_expr.baml:18:32 (type) len=6 "string"
// match_expr.baml:19:3 (keyword) len=5 "match"
// match_expr.baml:19:10 (parameter) len=3 "pet"
// match_expr.baml:20:5 (variable) len=3 "cat"
// match_expr.baml:20:10 (class) len=3 "Cat"
// match_expr.baml:20:17 (variable) len=3 "cat"
// match_expr.baml:20:21 (property) len=4 "name"
// match_expr.baml:21:5 (variable) len=3 "dog"
// match_expr.baml:21:10 (class) len=3 "Dog"
// match_expr.baml:21:17 (variable) len=3 "dog"
// match_expr.baml:21:21 (property) len=4 "name"
// match_expr.baml:22:10 (string) len=1 "\""
// match_expr.baml:22:11 (string) len=5 "other"
// match_expr.baml:22:17 (string) len=3 "pet"
// match_expr.baml:22:20 (string) len=1 "\""
