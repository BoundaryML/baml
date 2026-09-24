client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

function Extract<T>(text: string) -> T {
    client: TestClient
    prompt: `Extract ${text}.\n${ctx.output_format()}`
}

function mismatch_is_catchable(value: reflect.AnyClass) -> bool throws never {
    let ignored = value.get<int>("first") catch (e) {
        reflect.errors.TypeMismatch { message } => {
            return message.includes("field `VetrecLike.first`");
        }
    };
    false
}

function main() -> bool {
    let optional_string = reflect.Type.of<string>().optional().as_type();
    let runtime_t = reflect.class.new(
        "VetrecLike",
        {
            "first": optional_string.meta(description = "first field"),
            "second": optional_string,
            "third": optional_string,
            "fourth": optional_string,
            "fifth": optional_string,
        },
    );
    type Runtime = unreflect(runtime_t.as_type())
    let opaque = Extract@parse<Runtime>(`{"first":"one","second":"two","third":null,"fourth":"four","fifth":"five"}`);
    let record: reflect.AnyClass = opaque else {
        throw "Expected class"
    };

    let first_field: reflect.class.Field = record.get_field("first") else {
        throw "Expected first field"
    };
    let fields = record.list_fields();
    record.name() == "VetrecLike"
        && record.type() == runtime_t.as_type()
        && record.attributes().alias == null
        && record.has_field("first")
        && record.has_field("third")
        && !record.has_field("missing")
        && record.get<string?>("first") == "one"
        && record.get<string?>("second") == "two"
        && record.get<string?>("third") == null
        && record.get<string?>("fourth") == "four"
        && record.get<string?>("fifth") == "five"
        && record.get<string?>("missing") == null
        && fields.length() == 5
        && first_field.name == "first"
        && first_field.type == optional_string
        && first_field.metadata().alias == null
        && first_field.value<string?>() == "one"
        && mismatch_is_catchable(record)
}

test "runtime_minted_class_narrows_and_exercises_the_complete_surface" {
    assert.equal(main(), true)
}
