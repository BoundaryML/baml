//! `baml_code(v)`: BAML code that rebuilds a recorded type.
//!
//! Each call below passes a type built at run time to `Pick`. The test
//! renders every recorded type with `baml_code`, runs the code in a second
//! program that makes the same calls, and checks that the second recording
//! names the same definitions: the code rebuilt exactly the recorded types.

mod support;

use std::fmt::Write as _;

use serde_json::Value as Json;
use support::*;

const DECLARED: &str = r#"
class Resume {
    name: string @description("Full name")
}

enum Mood {
    Happy
    Sad @description("Not happy")
}

function Pick<T>(x: T?) -> int { 1 }
function Echo<T>(x: T) -> T { x }
"#;

const BUILT: &str = r#"
function main(n: int) -> int {
    let row = reflect.Class.new("Row", {
        "name": string.type().meta(description = "Full \"name\"\nand ${more}\u{7}", alias = "full_name"),
        "age": int.type().optional(),
        "owner": Resume.type(),
        "mood": Mood.type(),
        "tags": reflect.Type.of<string[]>().meta(other = {"source": "crm"}),
    });
    let type Row = row;
    Pick<Row>(null);

    let cell = reflect.Class.new("Cell", {
        "row": row,
        "rows": reflect.Type.of<map<string, Row[]>>(),
    });
    let type Cell = cell;
    Pick<Cell[]>(null);

    let node = reflect.Class.builder("Node");
    node.field("value", int.type())
        .field("next", node.type().optional().meta(description = "The next node"))
        .field("children", node.type().array())
        .field("either", node.type().union([string.type(), Resume.type()]));
    let type Node = node.build();
    Pick<Node>(null);

    let step = reflect.Class.builder("Step");
    let plan = reflect.Class.builder("Plan");
    step.field("plan", plan.type()).field("label", string.type());
    plan.field("steps", step.type().array()).field("first", step.type().optional());
    let step_t = step.build();
    let type Plan = plan.build();
    Pick<Plan>(null);

    let type Color = reflect.Enum.new("Color", ["Red", reflect.Enum.value("Blue", description = "Sky")]);
    Pick<Color | Row>(null);

    let type OtherRow = reflect.Class.new("Row", { "other": int.type() });
    Pick<map<string, Row | OtherRow>>(null);

    let type Small = reflect.Class.new("Small", { "v": int.type() });
    let small = Echo<Small>(baml.json.from_string<Small>(`{"v": 1}`));
    n
}
"#;

/// The raw definition ids in `value`, in document order, with repeats.
fn ids(value: &Json) -> Vec<String> {
    fn walk(value: &Json, out: &mut Vec<String>) {
        match value {
            Json::Object(map) => {
                for (key, item) in map {
                    match (key.as_str(), item) {
                        ("def_id" | "$def_id", Json::String(id)) => out.push(id.clone()),
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

fn text(value: &Json) -> &str {
    value
        .as_str()
        .unwrap_or_else(|| panic!("expected code, got {value}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recorded_runtime_types_rebuild_from_their_code() {
    let first = tempfile::tempdir().unwrap();
    let source = format!("{DECLARED}{BUILT}");
    let results = record_program(first.path(), &source, &["Pick", "Echo"], &[("main", 1)]).await;
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    let mut first_index = index(first.path());
    let recorded = sql(
        &mut first_index,
        "SELECT type_args, baml_code(type_args['T']) FROM spans WHERE span_name = 'user.Pick' \
         ORDER BY start_time",
    );
    assert_eq!(recorded.rows.len(), 6);

    // The simplest case, spelled out.
    assert_eq!(
        text(&recorded.rows[0][1]),
        r#"{
    let type Row = reflect.Class.new("Row", {
        "name": reflect.Type.of<string>().meta(alias = "full_name", description = "Full \"name\"\nand ${more}\u{7}"),
        "age": reflect.Type.of<int | null>(),
        "owner": reflect.Type.of<Resume>(),
        "mood": reflect.Type.of<Mood>(),
        "tags": reflect.Type.of<string[]>().meta(other = {"source": "crm"}),
    });
    reflect.Type.of<Row>()
}"#
    );

    // Run every rendering, and make the same calls with what it builds.
    let mut rebuilt = String::from("function main(n: int) -> int {\n");
    for (at, row) in recorded.rows.iter().enumerate() {
        write!(
            rebuilt,
            "    let t{at} = {};\n    let type T{at} = t{at};\n    Pick<T{at}>(null);\n",
            text(&row[1])
        )
        .unwrap();
    }
    rebuilt.push_str("    n\n}\n");
    let second = tempfile::tempdir().unwrap();
    let results = record_program(
        second.path(),
        &format!("{DECLARED}{rebuilt}"),
        &["Pick"],
        &[("main", 1)],
    )
    .await;
    assert!(results.iter().all(Result::is_ok), "{rebuilt}\n{results:?}");
    let mut again = index(second.path());
    let replayed = sql(
        &mut again,
        "SELECT type_args FROM spans WHERE span_name = 'user.Pick' ORDER BY start_time",
    );
    assert_eq!(replayed.rows.len(), recorded.rows.len());
    for (recorded, replayed) in recorded.rows.iter().zip(&replayed.rows) {
        let expected = ids(&recorded[0]);
        assert!(!expected.is_empty());
        assert_eq!(
            ids(&replayed[0]),
            expected,
            "{}\n{}\n{}",
            text(&recorded[1]),
            recorded[0],
            replayed[0]
        );
    }

    // An instance's code is its class's; other values have none.
    let echo = sql(
        &mut first_index,
        "SELECT baml_code(type_args['T']), baml_code(output_value), \
         baml_code(output_value['v']), baml_code(input_args) \
         FROM spans WHERE span_name = 'user.Echo'",
    );
    assert_eq!(echo.rows[0][0], echo.rows[0][1]);
    assert!(text(&echo.rows[0][1]).contains(r#"reflect.Class.new("Small""#));
    assert_eq!(echo.rows[0][2], Json::Null);
    assert_eq!(echo.rows[0][3], Json::Null);
}
