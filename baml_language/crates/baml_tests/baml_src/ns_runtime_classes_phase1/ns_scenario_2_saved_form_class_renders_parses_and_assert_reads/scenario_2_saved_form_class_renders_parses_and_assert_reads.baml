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

function NoteType(saved: SavedField[]) -> reflect.class.Type {
    let fields: map<string, reflect.Type | reflect.WithMeta<reflect.Type>> = {};
    for (let field in saved) {
        let ty = match (field.kind) {
            "dropdown" => {
                let members: reflect.Type[] = [];
                for (let option in field.options) {
                    members.push(reflect.literal.new(option).as_type())
                }
                reflect.union.new(members).as_type()
            },
            "bulleted_list" => reflect.Type.of<string>().array().as_type(),
            "number" => reflect.Type.of<int>(),
            "text" => reflect.Type.of<string>(),
            _ => throw UnknownFieldKind { kind: field.kind },
        };
        let description = if (field.kind == "bulleted_list") {
            "use short phrases; " + field.description
        } else {
            field.description
        };
        fields.set(field.name, ty.meta(description = description))
    }
    reflect.class.new("VisitNote", fields)
}

function main() -> string {
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
    type Note = unreflect(note_t.as_type())
    let prompt = ExtractNote@render_prompt<Note>("sample").text();
    let note = ExtractNote@parse<Note>(`{"height_cm": 183, "chief_complaint": "cough", "bullets": ["dry", "night"]}`);
    let height = reflect.class.get_field<int>(note, "height_cm");
    let complaint = reflect.class.get_field<string>(note, "chief_complaint");
    let bullets = reflect.class.get_field<string[]>(note, "bullets");
    return prompt
        + "\n<RESULT>"
        + height.to_string()
        + "|"
        + complaint
        + "|"
        + bullets[1]
        + "|"
        + baml.json.to_string(note);
}

test "scenario_2_saved_form_class_renders_parses_and_assert_reads" {
    let result = main();
    let height = result.index_of("height_cm") ?? throw "height field should render";
    let complaint = result.index_of("chief_complaint") ?? throw "complaint field should render";
    let bullets = result.index_of("bullets") ?? throw "bullets field should render";
    assert.is_true(height < complaint && complaint < bullets);
    assert.is_true(
        result.ends_with(
            "<RESULT>183|cough|night|{\"height_cm\":183,\"chief_complaint\":\"cough\",\"bullets\":[\"dry\",\"night\"]}",
        ),
    )
}
