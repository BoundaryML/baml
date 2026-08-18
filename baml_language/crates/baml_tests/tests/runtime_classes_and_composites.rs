//! BEP-066 Scenarios 2 and 3: runtime classes and composites flow
//! through offline LLM companions, keep pointing at their declarations, and
//! remain usable through the dynamic access/JSON surfaces.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn scenario_2_saved_form_class_renders_parses_and_assert_reads() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        function ExtractNote<T>(transcript: string) -> T {
            client: TestClient
            prompt: `Extract a visit note from ${transcript}.\n${ctx.output_format}`
        }

        class SavedField {
            name string
            kind string
            options string[]
            description string
        }

        class UnknownFieldKind {
            kind string
        }

        function NoteType(saved: SavedField[]) -> baml.reflect.class.Type {
            let fields: map<string, type | baml.reflect.WithMeta<type>> = {}
            for (let field in saved) {
                let ty = match (field.kind) {
                    "dropdown" => {
                        let members: type[] = []
                        for (let option in field.options) {
                            members.push(reflect.literal.new(option).as_type())
                        }
                        reflect.union.new(members).as_type()
                    },
                    "bulleted_list" => type.of<string>().array().as_type(),
                    "number" => type.of<int>(),
                    "text" => type.of<string>(),
                    _ => throw UnknownFieldKind { kind: field.kind },
                }
                let description = if (field.kind == "bulleted_list") {
                    "use short phrases; " + field.description
                } else {
                    field.description
                }
                fields.set(field.name, ty.meta(description = description))
            }
            reflect.class.new("VisitNote", fields)
        }

        function main() -> string {
            let note_t = NoteType([
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
            ])
            let prompt = ExtractNote$render_prompt<unreflect(note_t.as_type())>("sample").text()
            let note = ExtractNote$parse<unreflect(note_t.as_type())>(
                `{"height_cm": 183, "chief_complaint": "cough", "bullets": ["dry", "night"]}`,
            )
            let height = reflect.class.get_field<int>(note, "height_cm")
            let complaint = reflect.class.get_field<string>(note, "chief_complaint")
            let bullets = reflect.class.get_field<string[]>(note, "bullets")
            return prompt
                + "\n<RESULT>" + height.to_string()
                + "|" + complaint
                + "|" + bullets[1]
                + "|" + baml.json.encode(note)
        }
        "##
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("runtime class render, SAP landing, reads, and JSON encode should succeed")
    else {
        panic!("expected a string result")
    };
    let height = result
        .find("height_cm")
        .expect("height field should render");
    let complaint = result
        .find("chief_complaint")
        .expect("complaint field should render");
    let bullets = result.find("bullets").expect("bullets field should render");
    assert!(
        height < complaint && complaint < bullets,
        "field order changed: {result}"
    );
    assert!(
        result.ends_with(
            r#"<RESULT>183|cough|night|{"height_cm":183,"chief_complaint":"cough","bullets":["dry","night"]}"#,
        ),
        "parsed values or concrete JSON encoding changed: {result}"
    );
}

#[tokio::test]
async fn scenario_3_tool_union_dispatches_by_runtime_class() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        function PickAction<T>(context: string) -> T {
            client: TestClient
            prompt: `Pick one action for ${context}.\n${ctx.output_format}`
        }

        function main() -> string {
            let read_args_t = reflect.class.new("Tool0Args", {
                "path": type.of<string>().meta(alias = "file_path"),
            })
            let read_t = reflect.class.new("Tool0Action", {
                "tool": reflect.literal.new("filesystem/read_file").as_type(),
                "args": read_args_t.as_type(),
            })
            let search_args_t = reflect.class.new("Tool1Args", {
                "query": type.of<string>(),
                "limit": type.of<int>().optional().as_type(),
            })
            let search_t = reflect.class.new("Tool1Action", {
                "tool": reflect.literal.new("web/search").as_type(),
                "args": search_args_t.as_type(),
            })
            let action_t = reflect.union.new([read_t.as_type(), search_t.as_type()])
            let prompt = PickAction$render_prompt<unreflect(action_t.as_type())>("read the file").text()
            let action = PickAction$parse<unreflect(action_t.as_type())>(
                `{"tool": "filesystem/read_file", "args": {"file_path": "/tmp/a.txt"}}`,
            )

            let branch = "none"
            if action is unreflect(read_t.as_type()) {
                branch = "read"
            } else if action is unreflect(search_t.as_type()) {
                branch = "search"
            }
            let matched = match (action) {
                unreflect(read_t.as_type()) => "read",
                unreflect(search_t.as_type()) => "search",
                _ => "none",
            }
            let args = reflect.class.get_field<unknown>(action, "args")
            let path = reflect.class.get_field<string>(args, "path")
            let tool = reflect.class.get_field<string>(action, "tool")
            return prompt
                + "\n<RESULT>" + branch
                + "|" + matched
                + "|" + (type.of_value(action) == read_t.as_type()).to_string()
                + "|" + tool
                + "|" + path
                + "|" + baml.json.encode(action)
        }
        "##
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("runtime tool-union render, parse, and identity dispatch should succeed")
    else {
        panic!("expected a string result")
    };
    assert!(
        result.contains("filesystem/read_file"),
        "literal missing: {result}"
    );
    assert!(
        result.contains("web/search"),
        "second branch missing: {result}"
    );
    assert!(
        result.ends_with(
            r#"<RESULT>read|read|true|filesystem/read_file|/tmp/a.txt|{"tool":"filesystem/read_file","args":{"path":"/tmp/a.txt"}}"#,
        ),
        "identity dispatch or nested landing changed: {result}"
    );
}

#[tokio::test]
async fn empty_runtime_union_throws_the_reserved_diagnostic() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let result = reflect.union.new([]) catch (e) {
                baml.reflect.errors.CompilationError => {
                    e.diagnostics[0].code + "|" + e.diagnostics[0].message
                }
            }
            if result is string {
                return result
            }
            return "constructor did not throw"
        }
        "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "E0160|a runtime union must contain at least one member".into()
        ))
    );
}

#[tokio::test]
async fn class_order_identity_composites_and_to_baml_are_canonical() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let left = reflect.class.new("Ordered", {
                "a": type.of<int>(),
                "b": type.of<string>().meta(description = "second"),
            })
            let right = reflect.class.new("Ordered", {
                "b": type.of<string>().meta(description = "second"),
                "a": type.of<int>(),
            })
            let literal = reflect.literal.new("fixed")
            let union = reflect.union.new([literal.as_type(), type.of<int>()])
            let map_t = reflect.map.new(type.of<string>(), union.as_type())
            let array_t = map_t.as_type().array()
            let optional_t = array_t.as_type().optional()
            let empty = reflect.class.new("Empty", {})
            let source = left.as_type().to_baml()
            return source
                + "\n<RIGHT>\n" + right.as_type().to_baml()
                + "\n<OPTIONAL>\n" + optional_t.as_type().to_baml()
                + "\n<FLAGS>" + (left != right).to_string()
                + "|" + (source == left.as_type().to_baml()).to_string()
                + "|" + (literal.as_type().as_literal() != null).to_string()
                + "|" + (union.as_type().as_union() != null).to_string()
                + "|" + (map_t.as_type().as_map() != null).to_string()
                + "|" + (array_t.as_type().as_array() != null).to_string()
                + "|" + (optional_t.as_type().as_union() != null).to_string()
                + "|" + (empty.fields().length() == 0).to_string()
        }
        "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            concat!(
                "class Ordered {\n",
                "  a int\n",
                "  b string @description(\"second\")\n",
                "}\n",
                "<RIGHT>\n",
                "class Ordered {\n",
                "  b string @description(\"second\")\n",
                "  a int\n",
                "}\n",
                "<OPTIONAL>\n",
                "type RuntimeType = map<string, \"fixed\" | int>[]?\n",
                "<FLAGS>true|true|true|true|true|true|true|true"
            )
            .into()
        ))
    );
}

#[tokio::test]
async fn constructed_type_to_baml_compiles_to_equivalent_new_identity() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        function RoundTrip<T>() -> T {
            client: TestClient
            prompt: `${ctx.output_format}`
        }

        function main() -> bool throws unknown {
            let scores_t = reflect.map.new(
                type.of<string>(),
                type.of<int>().optional().as_type(),
            )
            let original = reflect.class.new("RoundTripRecord", {
                "kind": reflect.literal.new("visit").as_type().meta(
                    alias = "wire_kind",
                    description = "dispatch key",
                ),
                "tags": type.of<string>().array().as_type().meta(
                    description = "ordered labels",
                ),
                "scores": scores_t.as_type(),
            })
            let source = original.as_type().to_baml()
            let package = reflect.Package.compile({ "round_trip.baml": source })
            let compiled = package.get_class("root.RoundTripRecord")
                ?? throw "missing compiled RoundTripRecord"
            let document = `{
                "wire_kind": "visit",
                "tags": ["urgent", "review"],
                "scores": {"priority": 7, "followup": null}
            }`
            let original_value = RoundTrip$parse<unreflect(original.as_type())>(document)
            let compiled_value = RoundTrip$parse<unreflect(compiled.as_type())>(document)

            return original.as_type() != compiled.as_type()
                && source == compiled.as_type().to_baml()
                && RoundTrip$render_prompt<unreflect(original.as_type())>().text()
                    == RoundTrip$render_prompt<unreflect(compiled.as_type())>().text()
                && baml.json.encode(original_value) == baml.json.encode(compiled_value)
                && type.of_value(original_value) == original.as_type()
                && type.of_value(compiled_value) == compiled.as_type()
        }
        "##
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn runtime_class_readback_preserves_exact_type_and_all_metadata() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let literal = reflect.literal.new("fixed")
            let schema = reflect.class.new("Schema", {
                "tag": literal.as_type().meta(
                    alias = "wire_tag",
                    description = "dispatch key",
                    docstring = "source docs",
                    other = { "owner": "slice3" },
                ),
            })
            let field = schema.fields()[0]
            field.name == "tag"
                && field.type == literal.as_type()
                && field.meta.alias == "wire_tag"
                && field.meta.description == "dispatch key"
                && field.meta.docstring == "source docs"
                && field.meta.other.get("owner") == "slice3"
        }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn runtime_class_validation_is_eager_and_uses_compiler_diagnostics() {
    let output = baml_test!(
        r#"
        function main() -> string throws never {
            let bad_name = reflect.class.new("bad name", {}) catch (e) {
                baml.reflect.errors.CompilationError => e.diagnostics[0].code
            }
            let duplicate_wire_key = reflect.class.new("Collision", {
                "wire": type.of<string>(),
                "internal": type.of<int>().meta(alias = "wire"),
            }) catch (e) {
                baml.reflect.errors.CompilationError => e.diagnostics[0].code
            }
            let name_code = "name did not throw"
            if bad_name is string {
                name_code = bad_name
            }
            let key_code = "key did not throw"
            if duplicate_wire_key is string {
                key_code = duplicate_wire_key
            }
            return name_code + "|" + key_code
        }
        "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("E0010|E0149".into()))
    );
}

#[tokio::test]
async fn same_fields_in_different_orders_render_independently() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        function Render<T>() -> T {
            client: TestClient
            prompt: `${ctx.output_format}`
        }

        function main() -> string {
            let left = reflect.class.new("SameName", {
                "first": type.of<int>(),
                "second": type.of<string>(),
            })
            let right = reflect.class.new("SameName", {
                "second": type.of<string>(),
                "first": type.of<int>(),
            })
            return Render$render_prompt<unreflect(left.as_type())>().text()
                + "\n<RIGHT>\n"
                + Render$render_prompt<unreflect(right.as_type())>().text()
                + "\n<UNEQUAL>" + (left != right).to_string()
        }
        "##
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("both runtime-class overlays should render")
    else {
        panic!("expected a string result")
    };
    let (left, rest) = result
        .split_once("\n<RIGHT>\n")
        .expect("missing right prompt separator");
    let (right, unequal) = rest
        .split_once("\n<UNEQUAL>")
        .expect("missing identity separator");
    assert!(
        left.find("first").unwrap() < left.find("second").unwrap(),
        "left prompt lost insertion order: {left}"
    );
    assert!(
        right.find("second").unwrap() < right.find("first").unwrap(),
        "right prompt lost insertion order: {right}"
    );
    assert_eq!(unequal, "true", "construction identity was structural");
}

#[tokio::test]
async fn get_field_missing_and_wrong_type_throw_compilation_diagnostics() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        function Extract<T>() -> T {
            client: TestClient
            prompt: `${ctx.output_format}`
        }

        function main() -> string {
            let t = reflect.class.new("OneField", { "count": type.of<int>() })
            let value = Extract$parse<unreflect(t.as_type())>(`{"count": 4}`)
            let missing = reflect.class.get_field<int>(value, "absent") catch (e) {
                baml.reflect.errors.CompilationError => {
                    e.diagnostics[0].code + ":" + e.diagnostics[0].message
                }
            }
            let wrong = reflect.class.get_field<string>(value, "count") catch (e) {
                baml.reflect.errors.CompilationError => {
                    e.diagnostics[0].code + ":" + e.diagnostics[0].message
                }
            }
            let missing_text = "missing read did not throw"
            if missing is string {
                missing_text = missing
            }
            let wrong_text = "wrong read did not throw"
            if wrong is string {
                wrong_text = wrong
            }
            return missing_text + "|" + wrong_text
        }
        "##
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("assert-read errors should be catchable compiler diagnostics")
    else {
        panic!("expected a string result")
    };
    assert!(
        result.starts_with("E0001:class `OneField` has no field `absent`|E0001:"),
        "unexpected diagnostic: {result}"
    );
    assert!(
        result.contains("field `OneField.count` has type `int`, expected `string`"),
        "unexpected type mismatch: {result}"
    );
}
