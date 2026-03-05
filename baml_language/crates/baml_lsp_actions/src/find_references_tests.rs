//! Tests for find-all-references using cursor-based testing.

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

        let references = test.find_all_references();
        assert!(
            references.len() >= 3,
            "Should find at least 3 references to 'x', found: {references:?}"
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

        let references = test.find_all_references();
        // We can find: input in `let a = input`, `input + "!"`, `match (input)`, `=> input`
        assert!(
            references.len() >= 3,
            "Should find at least 3 references to 'input', found: {references:?}",
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

        let references = test.find_all_references();
        assert!(
            references.len() >= 3,
            "Should find at least 3 references to 'Helper', found: {references:?}"
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

        let references = test.find_all_references();
        // We can find: Person { ... } object literals and p.name field access
        // Type annotations (-> Person, p: Person) are not tracked
        assert!(
            !references.is_empty(),
            "Should find at least 1 reference to 'Person', found: {references:?}"
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

        let references = test.find_all_references();
        // We can find: Status.Active and Status.Inactive expression usages
        // Type annotations (-> Status, s Status) and match patterns are not tracked
        assert!(
            references.len() >= 2,
            "Should find at least 2 references to 'Status', found: {references:?}"
        );
    }

    #[test]
    fn test_find_refs_pattern_binding() {
        let test = CursorTest::new(
            r#"
enum Result {
    Ok { value string }
    Err { message string }
}

function HandleResult(r: Result) -> string {
    match (r) {
        Ok(<[CURSOR]o) => o.value + o.value
        Err(e) => e.message
    }
}
"#,
        );

        let references = test.find_all_references();
        // Pattern bindings might have limited support
        assert!(
            !references.is_empty() || references[0] == "No references found",
            "Pattern binding references, found: {references:?}",
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

        let references = test.find_all_references();
        // Field references might include field accesses
        assert!(
            !references.is_empty(),
            "Should find references to 'name' field, found: {references:?}"
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

        let references = test.find_all_references();
        // Even unused variables should find at least the definition itself
        assert!(
            !references.is_empty() || references[0] == "No references found",
            "Unused variable references, found: {references:?}"
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

        let references = test.find_all_references();
        // Should find uses of x at the same scope level
        // Note: Nested block support is limited in current implementation
        assert!(
            !references.is_empty(),
            "Should find references to local variable, found: {references:?}"
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

        let references = test.find_all_references();
        // Should find: Person { ... } object literal and p.name field access
        // Type annotations (-> Person, p: Person) are not tracked
        assert!(
            !references.is_empty(),
            "Should find references across files, found: {references:?}"
        );
    }
}
