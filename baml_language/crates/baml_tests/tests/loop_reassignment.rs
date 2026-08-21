//! Regression tests for outer-variable reassignment across loop branches.

use baml_tests::{baml_test, baml_test_optimized};
use bex_engine::BexExternalValue;

const SOURCE: &str = r###"
        class Fact {
            subject_id string
            section string
            text string
        }

        function parse_subject_file(content: string, subject_id: string) -> Fact[] {
            let facts: Fact[] = []
            let current_section = ""
            let in_scope = false
            for (let raw_line in content.lines()) {
                let line = raw_line.trim()
                if (line.starts_with("## ")) {
                    let heading = line.slice(3, line.length())
                    in_scope = heading != "One-Sentence Takeaway" && heading != "References & Rabbit Holes"
                    current_section = heading
                } else if (in_scope && line.starts_with("- ")) {
                    facts.push(Fact {
                        subject_id: subject_id,
                        section: current_section,
                        text: line.slice(2, line.length()),
                    })
                }
            }
            facts
        }

        function main() -> string {
            let content = #"# Created Pipeline (Marketing)

---

## 1. First Idea

- Bullet one here.

## One-Sentence Takeaway

- This bullet should be ignored.

## 2. Second Idea

- Bullet two here.
"#
            let facts = parse_subject_file(content, "x")
            facts[0].section + ":" + facts[0].text + "|" + facts[1].section + ":" + facts[1].text
        }
        "###;

fn expected_result() -> Result<BexExternalValue, String> {
    Ok(BexExternalValue::String(
        "1. First Idea:Bullet one here.|2. Second Idea:Bullet two here."
            .to_string()
            .into(),
    ))
}

#[tokio::test]
async fn multiple_outer_reassignments_with_array_push() {
    let output = baml_test!(SOURCE);

    assert_eq!(
        output.result.map_err(|error| format!("{error:?}")),
        expected_result()
    );
}

#[tokio::test]
async fn multiple_outer_reassignments_with_array_push_optimized() {
    let output = baml_test_optimized!(SOURCE);

    assert_eq!(
        output.result.map_err(|error| format!("{error:?}")),
        expected_result()
    );
}
