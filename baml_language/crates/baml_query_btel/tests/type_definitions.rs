//! The A/B contract for recorded type definitions (see
//! `baml_language/docs/dyn-type-definitions/SPEC.md`).
//!
//! Every class or enum a captured value names carries a definition id, and
//! each rendered cell includes a definition the first time it names it.
//! Both implementations must pass these tests unchanged. Definition ids are
//! opaque: the assertions rename them `d0`, `d1`, … in order of first
//! appearance within a cell, and compare raw ids only for identity.

mod support;

use serde_json::{Map, Value as Json, json};
use support::*;

/// Renames every `def`/`$def` id in `value` to `d0`, `d1`, … in order of
/// first appearance (document order), and returns the renamed value.
fn normalized(value: &Json) -> Json {
    fn walk(value: &Json, ids: &mut Vec<String>) -> Json {
        match value {
            Json::Object(map) => {
                let mut out = Map::new();
                for (key, item) in map {
                    let renamed = match (key.as_str(), item) {
                        ("def" | "$def", Json::String(id)) => {
                            let at = ids.iter().position(|seen| seen == id).unwrap_or_else(|| {
                                ids.push(id.clone());
                                ids.len() - 1
                            });
                            Json::String(format!("d{at}"))
                        }
                        _ => walk(item, ids),
                    };
                    out.insert(key.clone(), renamed);
                }
                Json::Object(out)
            }
            Json::Array(items) => Json::Array(items.iter().map(|item| walk(item, ids)).collect()),
            other => other.clone(),
        }
    }
    walk(value, &mut Vec::new())
}

/// The raw definition ids in `value`, in document order, with repeats.
fn raw_ids(value: &Json) -> Vec<String> {
    fn walk(value: &Json, out: &mut Vec<String>) {
        match value {
            Json::Object(map) => {
                for (key, item) in map {
                    match (key.as_str(), item) {
                        ("def" | "$def", Json::String(id)) => out.push(id.clone()),
                        _ => walk(item, out),
                    }
                }
            }
            Json::Array(items) => items.iter().for_each(|item| walk(item, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(value, &mut out);
    out
}

fn ty(node: Json) -> Json {
    let mut envelope = Map::new();
    envelope.insert("$type".into(), node);
    Json::Object(envelope)
}

const DECLARED: &str = r#"
class Resume {
    name: string @description("Full name"),
    age: int? @alias("years"),
    @@description("A candidate")
}
enum Status {
    Active @description("Currently employed"),
    Inactive @alias("off"),
}
class Box<T> {
    value: T,
    items: T[],
}
class Employee {
    name: string,
    manager: Employee?,
}
class Left {
    right: Right?,
}
class Right {
    left: Left?,
}
function Pick<T>(x: T) -> T { x }
function Echo(r: Resume, s: Status) -> Resume { r }
function main(n: int) -> int {
    let r = Resume { name: "ann", age: 3 };
    let a = Pick(r);
    let b = Pick<Box<Resume>>(Box<Resume> { value: r, items: [r] });
    let e = Pick(Employee { name: "e", manager: null });
    let l = Pick(Left { right: null });
    let s = Pick<Status>(Status.Active);
    let echoed = Echo(r, Status.Inactive);
    n
}
"#;

fn resume_definition() -> Json {
    json!({
        "kind": "class",
        "name": "user.Resume",
        "description": "A candidate",
        "fields": [
            {"name": "name", "schema": {"type": "string"}, "description": "Full name"},
            {"name": "age", "schema": {"type": "optional", "inner": {"type": "int"}}, "alias": "years"},
        ],
    })
}

fn status_definition() -> Json {
    json!({
        "kind": "enum",
        "name": "user.Status",
        "variants": [
            {"name": "Active", "description": "Currently employed"},
            {"name": "Inactive", "alias": "off"},
        ],
    })
}

/// Declared classes and enums: metadata, generics, nesting, self and mutual
/// recursion, and instances and enum values in a function's inputs and
/// output. Each cell names each definition once, at its first reference.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn declared_types_carry_their_definitions() {
    let project = tempfile::tempdir().unwrap();
    let results = record_program(project.path(), DECLARED, &["Pick", "Echo"], &[("main", 1)]).await;
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    let mut index = index(project.path());
    let rows = sql(
        &mut index,
        "SELECT type_args FROM spans WHERE span_name = 'user.Pick' ORDER BY start_time",
    );
    let cells: Vec<Json> = rows.rows.iter().map(|row| normalized(&row[0])).collect();
    let resume = json!({"type": "class", "name": "user.Resume", "def": "d0", "definition": resume_definition()});
    assert_eq!(
        cells,
        vec![
            json!({"T": ty(resume.clone())}),
            // The argument's definition comes first: `args` precede `definition`.
            json!({"T": ty(json!({
                "type": "class",
                "name": "user.Box",
                "args": [resume],
                "def": "d1",
                "definition": {
                    "kind": "class",
                    "name": "user.Box",
                    "type_params": 1,
                    "fields": [
                        {"name": "value", "schema": {"type": "typeParam", "index": 0}},
                        {"name": "items", "schema": {"type": "list", "item": {"type": "typeParam", "index": 0}}},
                    ],
                },
            }))}),
            // Self recursion: the inner reference is the id alone.
            json!({"T": ty(json!({
                "type": "class",
                "name": "user.Employee",
                "def": "d0",
                "definition": {
                    "kind": "class",
                    "name": "user.Employee",
                    "fields": [
                        {"name": "name", "schema": {"type": "string"}},
                        {"name": "manager", "schema": {"type": "optional", "inner":
                            {"type": "class", "name": "user.Employee", "def": "d0"}}},
                    ],
                },
            }))}),
            // Mutual recursion: Right is defined inside Left's definition.
            json!({"T": ty(json!({
                "type": "class",
                "name": "user.Left",
                "def": "d0",
                "definition": {
                    "kind": "class",
                    "name": "user.Left",
                    "fields": [{"name": "right", "schema": {"type": "optional", "inner": {
                        "type": "class",
                        "name": "user.Right",
                        "def": "d1",
                        "definition": {
                            "kind": "class",
                            "name": "user.Right",
                            "fields": [{"name": "left", "schema": {"type": "optional", "inner":
                                {"type": "class", "name": "user.Left", "def": "d0"}}}],
                        },
                    }}}],
                },
            }))}),
            json!({"T": ty(json!({
                "type": "enum",
                "name": "user.Status",
                "def": "d0",
                "definition": status_definition(),
            }))}),
        ]
    );

    let echo = sql(
        &mut index,
        "SELECT input_args, output_value FROM spans WHERE span_name = 'user.Echo'",
    );
    assert_eq!(
        normalized(&echo.rows[0][0]),
        json!({
            "r": {"$class": "user.Resume", "$def": "d0", "$definition": resume_definition(), "name": "ann", "age": 3},
            "s": {"$enum": "user.Status", "$variant": "Inactive", "$def": "d1", "$definition": status_definition()},
        })
    );
    assert_eq!(
        normalized(&echo.rows[0][1]),
        json!({"$class": "user.Resume", "$def": "d0", "$definition": resume_definition(), "name": "ann", "age": 3})
    );
    // One class, one id: within a recording, every reference to Resume has
    // the same id, whichever cell or capture it is in.
    let resume_ids: Vec<String> = [&rows.rows[0][0], &echo.rows[0][0], &echo.rows[0][1]]
        .into_iter()
        .map(|cell| raw_ids(cell)[0].clone())
        .collect();
    assert!(
        resume_ids.windows(2).all(|pair| pair[0] == pair[1]),
        "{resume_ids:?}"
    );
}

const RUNTIME: &str = r#"
function Pick<T>(x: T) -> T { x }
function Echo<T>(x: T) -> T { x }
function main(n: int) -> int {
    let b = reflect.class.builder("Person");
    b.field("name", reflect.Type.of<string>());
    b.field("age", reflect.Type.of<int>());
    let person_t = b.build();
    type Person = unreflect(person_t.as_type())

    let o = reflect.class.builder("Person");
    o.field("email", reflect.Type.of<string>());
    let other_t = o.build();
    type OtherPerson = unreflect(other_t.as_type())

    let p = baml.json.from_string<Person>(`{"name":"ann","age":3}`);
    let echoed = Echo<Person>(p);
    let other = Pick<OtherPerson?>(null);
    let both = Pick<map<string, Person | OtherPerson>>({});
    n
}
"#;

/// Classes built at run time: bare names, definitions in type arguments and
/// in instances, and distinct ids for distinct classes with the same name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runtime_classes_carry_their_definitions_and_identity() {
    let project = tempfile::tempdir().unwrap();
    let results = record_program(project.path(), RUNTIME, &["Pick", "Echo"], &[("main", 1)]).await;
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    let mut index = index(project.path());
    let person = json!({
        "kind": "class",
        "name": "Person",
        "fields": [
            {"name": "name", "schema": {"type": "string"}},
            {"name": "age", "schema": {"type": "int"}},
        ],
    });
    let other = json!({
        "kind": "class",
        "name": "Person",
        "fields": [{"name": "email", "schema": {"type": "string"}}],
    });

    let echo = sql(
        &mut index,
        "SELECT type_args, input_args, output_value FROM spans WHERE span_name = 'user.Echo'",
    );
    let instance =
        json!({"$class": "Person", "$def": "d0", "$definition": person, "name": "ann", "age": 3});
    assert_eq!(
        normalized(&echo.rows[0][0]),
        json!({"T": ty(json!({"type": "class", "name": "Person", "def": "d0", "definition": person}))})
    );
    assert_eq!(normalized(&echo.rows[0][1]), json!({"x": instance}));
    assert_eq!(normalized(&echo.rows[0][2]), instance);

    let picks = sql(
        &mut index,
        "SELECT type_args FROM spans WHERE span_name = 'user.Pick' ORDER BY start_time",
    );
    assert_eq!(
        normalized(&picks.rows[0][0]),
        json!({"T": ty(json!({"type": "optional", "inner":
            {"type": "class", "name": "Person", "def": "d0", "definition": other}}))})
    );
    // Same name, different classes: different ids in one cell.
    assert_eq!(
        normalized(&picks.rows[1][0]),
        json!({"T": ty(json!({
            "type": "map",
            "key": {"type": "string"},
            "value": {"type": "union", "variants": [
                {"type": "class", "name": "Person", "def": "d0", "definition": person},
                {"type": "class", "name": "Person", "def": "d1", "definition": other},
            ]},
        }))})
    );
    // And the same ids across cells of one recording.
    let echo_id = raw_ids(&echo.rows[0][0])[0].clone();
    let other_id = raw_ids(&picks.rows[0][0])[0].clone();
    let both = raw_ids(&picks.rows[1][0]);
    assert_eq!(both, vec![echo_id, other_id]);
}

const COLLECTED: &str = r#"
function Use<T>(x: T) -> int { 1 }
function MakeAndUse(i: int) -> int {
    let b = reflect.class.builder("Temp");
    b.field("i", reflect.Type.of<int>());
    let t = b.build();
    type Temp = unreflect(t.as_type())
    Use<Temp[]>([])
}
function main(n: int) -> int {
    let total = 0;
    let i = 0;
    while (i < 50) {
        total = total + MakeAndUse(i);
        baml.sys.collect_garbage();
        i = i + 1;
    }
    total
}
"#;

/// A runtime class collected right after the call that names it still has
/// its definition: nothing reads as unavailable.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn definitions_survive_collection_of_their_class() {
    let project = tempfile::tempdir().unwrap();
    let results = record_program(project.path(), COLLECTED, &["Use"], &[("main", 1)]).await;
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    let mut index = index(project.path());
    let uses = sql(
        &mut index,
        "SELECT type_args FROM spans WHERE span_name = 'user.Use'",
    );
    assert_eq!(uses.rows.len(), 50);
    assert_eq!(uses.outcome.status, baml_query_btel::Status::Complete);
    let temp = json!({"T": ty(json!({"type": "list", "item": {
        "type": "class",
        "name": "Temp",
        "def": "d0",
        "definition": {"kind": "class", "name": "Temp", "fields": [{"name": "i", "schema": {"type": "int"}}]},
    }}))});
    for row in &uses.rows {
        assert_eq!(normalized(&row[0]), temp);
    }
}

const QUIET: &str = r#"
class Resume {
    name: string,
}
function Pick<T>(x: T) -> T { x }
function main(n: int) -> int {
    let r = Pick(Resume { name: "ann" }, $trace = trace.span());
    n
}
"#;

/// A span that does not capture its inputs records neither type arguments
/// nor definitions (the #5145 rule extends to definitions).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn uncaptured_spans_record_no_definitions() {
    let project = tempfile::tempdir().unwrap();
    let results = record_program(project.path(), QUIET, &[], &[("main", 1)]).await;
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    let mut index = index(project.path());
    let pick = sql(
        &mut index,
        "SELECT COUNT(*), COUNT(type_args), COUNT(input_args) FROM spans WHERE span_name = 'user.Pick'",
    );
    assert_eq!(pick.rows, vec![vec![json!(1), json!(0), json!(0)]]);
}
