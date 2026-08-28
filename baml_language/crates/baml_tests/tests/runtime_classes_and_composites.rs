//! BEP-066 Scenarios 2 and 3: runtime classes and composites flow
//! through offline LLM companions, keep pointing at their declarations, and
//! remain usable through the dynamic access/JSON surfaces.

use baml_compiler_diagnostics::Severity;
use baml_tests::{
    baml_test,
    stdlib_prefix::{check_user_files, setup_test_db},
};
use bex_engine::{BexExternalAdt, BexExternalValue};
use bex_external_types::TaggedHeapHandleKind;

fn compile_errors(source: &str) -> Vec<(String, String)> {
    check_user_files(&setup_test_db(source))
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| (diagnostic.code().to_string(), diagnostic.message))
        .collect()
}

#[test]
fn unknown_get_field_method_is_rejected_at_compile_time() {
    let errors = compile_errors(
        r#"
        function inspect(value: unknown) -> string {
            value.get_field<string>("name")
        }
        "#,
    );

    assert!(
        errors.iter().any(|(code, message)| {
            code == "E0007" && message.contains("get_field") && message.contains("unknown")
        }),
        "missing unresolved-member diagnostic: {errors:#?}"
    );
}

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
            prompt: `Extract a visit note from ${transcript}.\n${ctx.output_format()}`
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

        function NoteType(saved: SavedField[]) -> reflect.class.Type {
            let fields: map<string, reflect.Type | reflect.WithMeta<reflect.Type>> = {}
            for (let field in saved) {
                let ty = match (field.kind) {
                    "dropdown" => {
                        let members: reflect.Type[] = []
                        for (let option in field.options) {
                            members.push(reflect.literal.new(option).as_type())
                        }
                        reflect.union.new(members).as_type()
                    },
                    "bulleted_list" => reflect.Type.of<string>().array().as_type(),
                    "number" => reflect.Type.of<int>(),
                    "text" => reflect.Type.of<string>(),
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
            type RuntimeNote = unreflect(note_t.as_type())
            let prompt = ExtractNote@spec<RuntimeNote>("sample").prompt().text()
            let note = ExtractNote@spec<RuntimeNote>("sample").parse(
                `{"height_cm": 183, "chief_complaint": "cough", "bullets": ["dry", "night"]}`,
            )
            let height = reflect.class.get_field<int>(note, "height_cm")
            let complaint = reflect.class.get_field<string>(note, "chief_complaint")
            let bullets = reflect.class.get_field<string[]>(note, "bullets")
            return prompt
                + "\n<RESULT>" + height.to_string()
                + "|" + complaint
                + "|" + bullets[1]
                + "|" + baml.json.to_string(note)
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
            prompt: `Pick one action for ${context}.\n${ctx.output_format()}`
        }

        function main() -> string {
            let read_args_t = reflect.class.new("Tool0Args", {
                "path": reflect.Type.of<string>().meta(alias = "file_path"),
            })
            let read_t = reflect.class.new("Tool0Action", {
                "tool": reflect.literal.new("filesystem/read_file").as_type(),
                "args": read_args_t.as_type(),
            })
            let search_args_t = reflect.class.new("Tool1Args", {
                "query": reflect.Type.of<string>(),
                "limit": reflect.Type.of<int>().optional().as_type(),
            })
            let search_t = reflect.class.new("Tool1Action", {
                "tool": reflect.literal.new("web/search").as_type(),
                "args": search_args_t.as_type(),
            })
            let action_t = reflect.union.new([read_t.as_type(), search_t.as_type()])
            type RuntimeAction = unreflect(action_t.as_type())
            let prompt = PickAction@spec<RuntimeAction>("read the file").prompt().text()
            let action = PickAction@spec<RuntimeAction>("read the file").parse(
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
                + "|" + (reflect.Type.of_value(action) == read_t.as_type()).to_string()
                + "|" + tool
                + "|" + path
                + "|" + baml.json.to_string(action)
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
                reflect.errors.CompilationError => {
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
                "a": reflect.Type.of<int>(),
                "b": reflect.Type.of<string>().meta(description = "second"),
            })
            let right = reflect.class.new("Ordered", {
                "b": reflect.Type.of<string>().meta(description = "second"),
                "a": reflect.Type.of<int>(),
            })
            let literal = reflect.literal.new("fixed")
            let union = reflect.union.new([literal.as_type(), reflect.Type.of<int>()])
            let map_t = reflect.map.new(reflect.Type.of<string>(), union.as_type())
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
            prompt: `${ctx.output_format()}`
        }

        function main() -> bool throws unknown {
            let scores_t = reflect.map.new(
                reflect.Type.of<string>(),
                reflect.Type.of<int>().optional().as_type(),
            )
            let original = reflect.class.new("RoundTripRecord", {
                "kind": reflect.literal.new("visit").as_type().meta(
                    alias = "wire_kind",
                    description = "dispatch key",
                ),
                "tags": reflect.Type.of<string>().array().as_type().meta(
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
            type OriginalRecord = unreflect(original.as_type())
            type CompiledRecord = unreflect(compiled.as_type())
            let original_value = RoundTrip@spec<OriginalRecord>().parse(document)
            let compiled_value = RoundTrip@spec<CompiledRecord>().parse(document)

            return original.as_type() != compiled.as_type()
                && source == compiled.as_type().to_baml()
                && RoundTrip@spec<OriginalRecord>().prompt().text()
                    == RoundTrip@spec<CompiledRecord>().prompt().text()
                && baml.json.to_string(original_value) == baml.json.to_string(compiled_value)
                && reflect.Type.of_value(original_value) == original.as_type()
                && reflect.Type.of_value(compiled_value) == compiled.as_type()
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
                reflect.errors.CompilationError => e.diagnostics[0].code
            }
            let duplicate_wire_key = reflect.class.new("Collision", {
                "wire": reflect.Type.of<string>(),
                "internal": reflect.Type.of<int>().meta(alias = "wire"),
            }) catch (e) {
                reflect.errors.CompilationError => e.diagnostics[0].code
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
            prompt: `${ctx.output_format()}`
        }

        function main() -> string {
            let left = reflect.class.new("SameName", {
                "first": reflect.Type.of<int>(),
                "second": reflect.Type.of<string>(),
            })
            let right = reflect.class.new("SameName", {
                "second": reflect.Type.of<string>(),
                "first": reflect.Type.of<int>(),
            })
            type LeftRecord = unreflect(left.as_type())
            type RightRecord = unreflect(right.as_type())
            return Render@spec<LeftRecord>().prompt().text()
                + "\n<RIGHT>\n"
                + Render@spec<RightRecord>().prompt().text()
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
            prompt: `${ctx.output_format()}`
        }

        function main() -> string {
            let t = reflect.class.new("OneField", { "count": reflect.Type.of<int>() })
            type RuntimeOneField = unreflect(t.as_type())
            let value = Extract@spec<RuntimeOneField>().parse(`{"count": 4}`)
            let missing = reflect.class.get_field<int>(value, "absent") catch (e) {
                reflect.errors.CompilationError => {
                    e.diagnostics[0].code + ":" + e.diagnostics[0].message
                }
            }
            let wrong = reflect.class.get_field<string>(value, "count") catch (e) {
                reflect.errors.CompilationError => {
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

/// A runtime-created class has no spelling a host can resolve, and its bare
/// item name collides with a *compiled* class of the same name — an echoed
/// value would rebind to that declaration and violate its contract. So its
/// instance crosses as an opaque handle rather than structurally.
#[tokio::test]
async fn an_anonymous_class_instance_crosses_as_an_opaque_handle() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        /// The compiled declaration whose item name the runtime one repeats.
        class Widget {
            name: string,
        }

        function Extract<T>() -> T {
            client: TestClient
            prompt: `${ctx.output_format()}`
        }

        function main() -> unknown {
            let widget_t = reflect.class.new("Widget", {
                "name": reflect.Type.of<string>(),
            })
            type RuntimeWidget = unreflect(widget_t.as_type())
            Extract@spec<RuntimeWidget>().parse(`{"name":"anonymous"}`)
        }
        "##
    );

    match output
        .result
        .expect("a runtime class instance should reach the host")
    {
        BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
            kind: TaggedHeapHandleKind::RuntimeValue,
            ..
        }) => {}
        other => panic!(
            "an anonymous class instance must not cross structurally under a \
             name that resolves to the compiled `Widget`: {other:?}"
        ),
    }
}

/// The same for a *runtime-compiled package* member, which is the case a
/// name-based guard misses: it is a `Declared` name like any static class, and
/// emit qualifies it identically (`user.ExtractedRecord`), so it collides with
/// the statically compiled declaration below — whose fields even match, which
/// is what would make the rebind silent. Only the compile-time heap section is
/// host-addressable; a runtime-compiled member lives on the moving heap and has
/// no codegen entry, exactly like an anonymous one.
#[tokio::test]
async fn a_runtime_compiled_class_instance_crosses_as_an_opaque_handle() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        /// The statically compiled declaration a rebind would land on.
        class ExtractedRecord {
            account: string,
        }

        function Extract<T>() -> T {
            client: TestClient
            prompt: `${ctx.output_format()}`
        }

        function main() -> unknown throws unknown {
            let pkg = reflect.Package.compile({
                "schema.baml": "class ExtractedRecord { account string }"
            })
            let record_t = pkg.get_class("root.ExtractedRecord") ?? throw "missing ExtractedRecord"
            type RuntimeExtractedRecord = unreflect(record_t.as_type())
            Extract@spec<RuntimeExtractedRecord>().parse(`{"account":"AC-1"}`)
        }
        "##
    );

    match output
        .result
        .expect("a runtime-compiled class instance should reach the host")
    {
        BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
            kind: TaggedHeapHandleKind::RuntimeValue,
            ..
        }) => {}
        other => panic!(
            "a runtime-compiled class instance must not cross structurally under a \
             name that resolves to the static `ExtractedRecord`: {other:?}"
        ),
    }
}
