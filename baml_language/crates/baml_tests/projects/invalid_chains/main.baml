// Test invalid pattern chains — each link in a chain must be a subtype of the previous.

class Animal {
    name string
}

class Dog {
    name string
    breed string
}

// Incompatible chain links: string is not a subtype of int
function test_incompatible_types() -> string {
    let x: int: string = 5;
    return "ok";
}

// Class then incompatible type: int is not a subtype of Animal
function test_class_then_wrong_type() -> string {
    let x: Animal: int = Animal { name: "Rex" };
    return "ok";
}

// Class destructure then incompatible type
function test_destructure_then_wrong_type() -> string {
    let x: Animal { name }: int = Animal { name: "Rex" };
    return "ok";
}

// Type then class destructure of a different class
function test_type_then_wrong_destructure() -> string {
    let x: int: Dog { breed } = Dog { name: "Rex", breed: "Lab" };
    return "ok";
}

// Three-link chain: all incompatible
function test_three_link_chain() -> string {
    let x: int: string: bool = true;
    return "ok";
}
