//! Tests for `usages_at` using cursor-based testing.

#[cfg(test)]
mod tests {
    use crate::testing::CursorTest;

    #[test]
    fn test_find_refs_local_variable() {
        let test = CursorTest::new(
            r#"
function Test() -> string {
    let <[CURSOR]x = "hello"
    let y = x
    let z = x + " world"
    x
}
"#,
        );

        let usages = test.find_all_usages();
        assert!(
            usages.len() >= 3,
            "Should find at least 3 usages of 'x', found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_parameter() {
        let test = CursorTest::new(
            r#"
function Process(<[CURSOR]input: string) -> string {
    let a = input
    let b = input + "!"
    match (input) {
        "test" => input
        _ => "default"
    }
}
"#,
        );

        let usages = test.find_all_usages();
        assert!(
            usages.len() >= 3,
            "Should find at least 3 usages of 'input', found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_function() {
        let test = CursorTest::new(
            r#"
function <[CURSOR]Helper(x: string) -> string {
    x + "!"
}

function Main() -> string {
    let a = Helper("test")
    Helper("another")
}

function Other() -> string {
    Helper("third")
}
"#,
        );

        let usages = test.find_all_usages();
        assert!(
            usages.len() >= 3,
            "Should find at least 3 usages of 'Helper', found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_class() {
        let test = CursorTest::new(
            r#"
class <[CURSOR]Person {
    name string
    age int
}

function CreatePerson() -> Person {
    Person { name: "Alice", age: 30 }
}

function ProcessPerson(p: Person) -> string {
    p.name
}
"#,
        );

        let usages = test.find_all_usages();
        assert!(
            !usages.is_empty(),
            "Should find at least 1 usage of 'Person', found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_enum() {
        let test = CursorTest::new(
            r#"
enum <[CURSOR]Status {
    Active
    Inactive
}

function GetStatus() -> Status {
    Status.Active
}

function UseStatus() -> Status {
    let s = Status.Active
    Status.Inactive
}
"#,
        );

        let usages = test.find_all_usages();
        assert!(
            usages.len() >= 2,
            "Should find at least 2 usages of 'Status', found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_field() {
        let test = CursorTest::new(
            r#"
class Person {
    <[CURSOR]name string
    age int
}

function GetName(p: Person) -> string {
    p.name
}

function SetName(p: Person, n: string) -> Person {
    Person { name: n, age: p.age }
}
"#,
        );

        let usages = test.find_all_usages();
        assert!(
            usages.len() >= 2,
            "Should find at least 2 usages of 'name' (field access + constructor), found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_no_references() {
        let test = CursorTest::new(
            r#"
function Test() -> string {
    let <[CURSOR]unused = "value"
    "other"
}
"#,
        );

        let usages = test.find_all_usages();
        // An unused variable should have zero usages (definition site is excluded).
        assert!(
            usages.is_empty(),
            "Unused variable should have no usages, found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_across_blocks() {
        let test = CursorTest::new(
            r#"
function Test() -> string {
    let <[CURSOR]x = "outer"
    let y = x
    let z = x + x
    x
}
"#,
        );

        let usages = test.find_all_usages();
        assert!(
            !usages.is_empty(),
            "Should find usages of local variable, found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_local_variable_ignores_shadowed_binding() {
        let test = CursorTest::new(
            r#"
function Test() -> string {
    let <[CURSOR]x = "outer"
    let y = x
    {
        let x = "inner"
        x
    }
    x
}
"#,
        );

        let usages = test.find_all_usages();
        assert_eq!(
            usages.len(),
            2,
            "Should only find usages of the outer 'x', found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_find_refs_multi_file() {
        let mut builder = CursorTest::builder();
        builder.source(
            "types.baml",
            r#"
class <[CURSOR]Person {
    name string
}
"#,
        );
        builder.source(
            "functions.baml",
            r#"
function CreatePerson() -> Person {
    Person { name: "Alice" }
}

function ProcessPerson(p: Person) -> string {
    p.name
}
"#,
        );
        let test = builder.build();

        let usages = test.find_all_usages();
        assert!(
            !usages.is_empty(),
            "Should find usages across files, found: {:?}",
            usages
                .iter()
                .map(|l| test.format_location_with_name(l))
                .collect::<Vec<_>>()
        );
    }
}
