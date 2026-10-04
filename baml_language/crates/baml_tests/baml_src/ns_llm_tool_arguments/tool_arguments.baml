// A tool reflected from a function (`ai.tools.tool(f)`) is called with the
// model's arguments, which arrive as JSON: a class is a map, an enum is a
// string, a list holds json. `Tool.call` decodes each argument into its
// parameter's type before it calls the function, reading the same names the
// tool's schema advertises (`@alias` included).
//
// Per the corpus convention every case body is a top-level `function` and the
// `test` block only asserts.

enum ToolArgGymType {
    COMMERCIAL
    GARAGE @alias("garage-gym")
}

class ToolArgAddress {
    city: string,
}

class ToolArgGym {
    name: string @alias("label"),
    gym_type: ToolArgGymType,
    address: ToolArgAddress?,
    tags: string[],
}

function tool_arg_enum(kind: ToolArgGymType) -> string {
    kind.to_string()
}

function tool_arg_literal(mode: "auto" | "manual") -> string {
    mode
}

function tool_arg_numbers(ratio: float, count: int) -> string {
    `${ratio}|${count}`
}

function tool_arg_string(note: string) -> string {
    note
}

function tool_arg_optional(note: string?) -> string {
    note ?? "null"
}

function tool_arg_list(tags: string[]) -> string {
    tags.join(",")
}

function tool_arg_map(extra: map<string, int>) -> string {
    `${extra.get("a") ?? 0}`
}

function tool_arg_class(gym: ToolArgGym) -> string {
    `${gym.name}|${gym.gym_type}|${gym.address?.city ?? "none"}|${gym.tags.join(",")}`
}

function tool_arg_classes(gyms: ToolArgGym[]) -> string {
    gyms.map((gym) -> { gym.name }).join(",")
}

function tool_arg_unknown(payload: unknown) -> string {
    baml.json.to_string(payload)
}

/// What `tool_arg_string` receives for `note`, decoded from the tool's JSON result.
function tool_arg_string_round_trip(note: string) -> string {
    let args: map<string, unknown> = { "note": note };
    baml.json.from_string<string>(ai.tools.tool(tool_arg_string).call(args))
}

/// Calls the tool reflected from `handler` with arguments written as JSON
/// text, as a provider delivers them. A failure renders as its message.
function tool_arg_call(handler: reflect.AnyFunction, arguments: string) -> string {
    let args = baml.json.from_string<map<string, unknown>>(arguments);
    ai.tools.tool(handler).call(args) catch (e) {
        baml.errors.InvalidArgument => `InvalidArgument: ${e.message}`,
    }
}

test "an enum argument decodes from its name or its alias" {
    assert.equal(tool_arg_call(tool_arg_enum, `{"kind": "COMMERCIAL"}`), `"COMMERCIAL"`);
    assert.equal(tool_arg_call(tool_arg_enum, `{"kind": "garage-gym"}`), `"GARAGE"`)
}

test "a literal argument decodes from its value" {
    assert.equal(tool_arg_call(tool_arg_literal, `{"mode": "auto"}`), `"auto"`)
}

test "number, string and null arguments keep their values" {
    assert.equal(tool_arg_call(tool_arg_numbers, `{"ratio": 2, "count": 3}`), `"2.0|3"`);
    assert.equal(tool_arg_call(tool_arg_string, `{"note": "hi"}`), `"hi"`);
    assert.equal(tool_arg_call(tool_arg_optional, `{"note": null}`), `"null"`)
}

test "a string argument is passed as written" {
    let note = "{\"a\": 1}, \"quotes\" and ```json\n{}\n```";
    assert.equal(tool_arg_string_round_trip(note), note)
}

test "list and map arguments decode into their element types" {
    assert.equal(tool_arg_call(tool_arg_list, `{"tags": ["a", "b"]}`), `"a,b"`);
    assert.equal(tool_arg_call(tool_arg_map, `{"extra": {"a": 7}}`), `"7"`)
}

test "a class argument decodes with its aliases, nested classes and enums" {
    assert.equal(
        tool_arg_call(
            tool_arg_class,
            `{"gym": {"label": "Home", "gym_type": "garage-gym", "address": {"city": "Oslo"}, "tags": ["x"]}}`,
        ),
        `"Home|GARAGE|Oslo|x"`,
    );
    assert.equal(
        tool_arg_call(
            tool_arg_classes,
            `{"gyms": [{"label": "A", "gym_type": "COMMERCIAL", "tags": []}, {"label": "B", "gym_type": "garage-gym", "tags": []}]}`,
        ),
        `"A,B"`,
    )
}

test "an unknown parameter receives the argument as it arrived" {
    let result = tool_arg_call(tool_arg_unknown, `{"payload": {"a": [1, null]}}`);
    assert.equal(baml.json.from_string<string>(result), `{"a":[1,null]}`)
}

test "an argument that does not fit its parameter is an InvalidArgument naming it" {
    let wrong_class = tool_arg_call(tool_arg_class, `{"gym": {"label": "Home"}}`);
    assert.is_true(wrong_class.starts_with("InvalidArgument: invalid arguments for tool_arg_class: gym: "));
    let wrong_literal = tool_arg_call(tool_arg_literal, `{"mode": "sometimes"}`);
    assert.is_true(wrong_literal.starts_with("InvalidArgument: invalid arguments for tool_arg_literal: "));
    let missing = tool_arg_call(tool_arg_string, `{}`);
    assert.is_true(missing.starts_with("InvalidArgument: invalid arguments for tool_arg_string: "));
    let extra = tool_arg_call(tool_arg_string, `{"note": "hi", "other": 1}`);
    assert.is_true(extra.starts_with("InvalidArgument: invalid arguments for tool_arg_string: "))
}

// The runner's path: a model's tool call reaches a handler that takes a class.

class ToolArgProbe {
    saved: string[],
}

class ToolArgScriptedClient {
    turns: int,

    implements ai.Client {
        function id(self) -> string {
            "tool-arguments-client"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request { method: "POST", url: "https://example.invalid/tool-arguments", headers: {}, body: "{}" }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            self.turns += 1;
            let content: ai.content.ModelBlock[] = [];
            if (self.turns == 1) {
                let args: map<string, unknown> = {
                    "gym": { "label": "Home", "gym_type": "garage-gym", "tags": ["barbell"] },
                };
                content.push(ai.content.ToolUse { id: "call_1", name: "save_gym", args: args });
            } else {
                content.push(ai.content.Text { text: "saved" });
            }
            ai.ModelTurn { calls: [], content: content, stop_reason: ai.content.StopReason.Complete, usage: null }
        }
    }
}

function ToolArgSaveGym(request: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `${request}`
}

function tool_arg_agent_run() -> string {
    let probe = ToolArgProbe { saved: [] };
    let save_gym = (gym: ToolArgGym) -> string {
        probe.saved.push(`${gym.name}:${gym.gym_type}`);
        "ok"
    };
    let spec = ToolArgSaveGym@spec("Save my home gym.");
    spec.toolbox = ai.tools.Toolbox.new([ai.tools.tool(save_gym, name = "save_gym", description = "Saves a gym.")]);
    let reply = ai.Agent.new(client = ToolArgScriptedClient { turns: 0 }).run(spec).value;
    `${reply}|${probe.saved.join(",")}`
}

test "the agent runs a tool whose parameter is a class" {
    assert.equal(tool_arg_agent_run(), "saved|Home:GARAGE")
}

// `baml.sap.can_parse` is how `Tool.call` knows which arguments to decode.

type ToolArgHandler = (string) -> string throws never;

function tool_arg_can_parse() -> string {
    [
        baml.sap.can_parse(reflect.Type.of<string>()),
        baml.sap.can_parse(reflect.Type.of<ToolArgGymType>()),
        baml.sap.can_parse(reflect.Type.of<ToolArgGym[]>()),
        baml.sap.can_parse(reflect.Type.of<map<string, int?>>()),
        baml.sap.can_parse(reflect.Type.of<"auto" | "manual">()),
        baml.sap.can_parse(reflect.Type.of<unknown>()),
        baml.sap.can_parse(reflect.Type.of<uint8array>()),
        baml.sap.can_parse(reflect.Type.of<ToolArgHandler>()),
        baml.sap.can_parse(reflect.Type.of<unknown[]>()),
    ]
        .map((able) -> { `${able}` })
        .join(",")
}

test "schema-aligned parsing reports the types it can model" {
    assert.equal(tool_arg_can_parse(), "true,true,true,true,true,false,false,false,false")
}
