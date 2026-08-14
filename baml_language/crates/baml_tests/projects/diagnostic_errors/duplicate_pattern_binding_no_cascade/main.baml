class Person {
    age int
    name string
}

function string_last(person: Person) -> int {
    let Person {
        age: let item,
        name: let item,
    } = person;
    item
}

function int_last(person: Person) -> string {
    let Person {
        name: let item,
        age: let item,
    } = person;
    item
}

function if_let_string_last(person: Person | null) -> int {
    if let Person {
        age: let item,
        name: let item,
    } = person {
        item
    } else {
        0
    }
}

function if_let_int_last(person: Person | null) -> string {
    if let Person {
        name: let item,
        age: let item,
    } = person {
        item
    } else {
        ""
    }
}
