client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

function ExtractNote<T>(transcript: string) -> T {
    client: TestClient
    prompt: `Extract a visit note from ${transcript}.\n${ctx.output_format()}`
}

class SavedField {
    name: string,
    kind: string,
    options: string[],
    description: string,
}

class UnknownFieldKind {
    kind: string,
}

function NoteType(saved: SavedField[]) -> reflect.Class {
    let fields: map<string, reflect.TypeView | reflect.WithMeta<reflect.Type>> = {};
    for (let field in saved) {
        let ty = match (field.kind) {
            "dropdown" => {
                let members: reflect.TypeView[] = [];
                for (let option in field.options) {
                    members.push(reflect.Literal.new(option))
                }
                reflect.Union.new(members).as_type()
            },
            "bulleted_list" => string.type().array().as_type(),
            "number" => int.type(),
            "text" => string.type(),
            _ => throw UnknownFieldKind { kind: field.kind },
        };
        let description = if (field.kind == "bulleted_list") {
            "use short phrases; " + field.description
        } else {
            field.description
        };
        fields.set(field.name, ty.meta(description = description))
    }
    reflect.Class.new("VisitNote", fields)
}

class ScenarioResult {
    text: string,
    note: baml.json.json,
}

function main() -> ScenarioResult {
    let note_t = NoteType(
        [
            SavedField {
                name: "height_cm",
                kind: "number",
                options: [],
                description: "height in centimeters; convert feet and inches",
            },
            SavedField {
                name: "chief_complaint",
                kind: "text",
                options: [],
                description: "the patient's main complaint",
            },
            SavedField {
                name: "bullets",
                kind: "bulleted_list",
                options: [],
                description: "symptoms",
            },
        ],
    );
    let type Note = note_t;
    let prompt = ExtractNote@render_prompt<Note>("sample").text();
    let note = ExtractNote@parse<Note>(`{"height_cm": 183, "chief_complaint": "cough", "bullets": ["dry", "night"]}`);
    let height = reflect.Class.get_field<int>(note, "height_cm");
    let complaint = reflect.Class.get_field<string>(note, "chief_complaint");
    let bullets = reflect.Class.get_field<string[]>(note, "bullets");
    return ScenarioResult {
        text: prompt + "\n<RESULT>" + height.to_string() + "|" + complaint + "|" + bullets[1],
        note: baml.json.from(note),
    };
}

test "scenario_2_saved_form_class_renders_parses_and_assert_reads" {
    let scenario = main();
    let result = scenario.text;
    let height = result.index_of("height_cm") ?? throw "height field should render";
    let complaint = result.index_of("chief_complaint") ?? throw "complaint field should render";
    let bullets = result.index_of("bullets") ?? throw "bullets field should render";
    assert.is_true(height < complaint && complaint < bullets);
    assert.is_true(result.ends_with("<RESULT>183|cough|night"));
    assert.equal(
        scenario.note,
        { "height_cm": 183, "chief_complaint": "cough", "bullets": ["dry", "night"] },
    )
}
