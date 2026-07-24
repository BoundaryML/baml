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
