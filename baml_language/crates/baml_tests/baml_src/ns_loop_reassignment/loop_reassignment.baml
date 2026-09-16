class Fact {
    subject_id: string,
    section: string,
    text: string,
}

function parse_subject_file(content: string, subject_id: string) -> Fact[] {
    let facts: Fact[] = [];
    let current_section = "";
    let in_scope = false;
    for (let raw_line in content.lines()) {
        let line = raw_line.trim();
        if (line.starts_with("## ")) {
            let heading = line.slice(3, line.length());
            in_scope = heading != "One-Sentence Takeaway" && heading != "References & Rabbit Holes";
            current_section = heading;
        } else if (in_scope && line.starts_with("- ")) {
            facts.push(
                Fact {
                    subject_id: subject_id,
                    section: current_section,
                    text: line.slice(2, line.length()),
                },
            );
        }
    }
    facts
}

test "multiple outer reassignments with array push" {
    let content = `
        # Created Pipeline (Marketing)

        ---

        ## 1. First Idea

        - Bullet one here.

        ## One-Sentence Takeaway

        - This bullet should be ignored.

        ## 2. Second Idea

        - Bullet two here.
    `;
    let facts = parse_subject_file(content, "x");
    assert.equal(
        `${facts[0].section}:${facts[0].text}|${facts[1].section}:${facts[1].text}`,
        "1. First Idea:Bullet one here.|2. Second Idea:Bullet two here.",
    )
}
