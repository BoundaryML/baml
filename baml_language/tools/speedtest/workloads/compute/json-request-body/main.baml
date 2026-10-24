// Serialize a provider request body: the class graph a chat client builds
// before each model call (the shape of OpenAI's Responses API), with message
// items mixing text and image parts, function-call replays, tools whose
// `parameters` are JSON Schema `json` values, and optional fields left null.
// The request is built once in setup(); a job writes it with
// `baml.json.stringify`, once as is and once with `omit_nulls`.
class InputText {
  type: string
  text: string
}

class InputImage {
  type: string
  image_url: string?
  file_id: string?
  detail: string?
}

type InputPart = InputText | InputImage

class MessageItem {
  role: string
  content: InputPart[]
}

class FunctionCallItem {
  type: string
  call_id: string
  name: string
  arguments: string
}

class FunctionCallOutputItem {
  type: string
  call_id: string
  output: string
}

type InputItem = MessageItem | FunctionCallItem | FunctionCallOutputItem

class FunctionTool {
  type: string
  name: string
  description: string
  parameters: json
  strict: bool
}

class Reasoning {
  effort: string?
  summary: string?
}

class Request {
  model: string
  instructions: string?
  input: InputItem[]
  tools: FunctionTool[]
  tool_choice: string?
  temperature: float?
  top_p: float?
  max_output_tokens: int
  parallel_tool_calls: bool
  reasoning: Reasoning?
  metadata: map<string, string>
  store: bool
  user: string?
  stream: bool
}

class Rng {
  seed: int
}

function draw(rng: Rng, n: int) -> int {
  rng.seed = rng.seed * 16807 % 2147483647;
  rng.seed % n
}

function sentence(rng: Rng, words: string[], count: int) -> string {
  let parts: string[] = [];
  for (let i = 0; i < count; i += 1) { parts.push(words[draw(rng, words.length())]); }
  parts.join(" ")
}

function tool_schema(rng: Rng, words: string[], fields: int) -> json {
  let properties: map<string, json> = {};
  let required: json[] = [];
  for (let f = 0; f < fields; f += 1) {
    let name = words[draw(rng, words.length())] + "_" + f.to_string();
    let kind = draw(rng, 4);
    if (kind == 0) {
      properties[name] = { "type": "string", "description": sentence(rng, words, 6) };
    } else if (kind == 1) {
      properties[name] = { "type": "integer", "minimum": 0, "maximum": 1000 + draw(rng, 9000) };
    } else if (kind == 2) {
      properties[name] = { "type": "array", "items": { "type": "string", "enum": ["low", "medium", "high"] } };
    } else {
      properties[name] = {
        "type": "object",
        "properties": { "lat": { "type": "number" }, "lng": { "type": "number" } },
        "required": ["lat", "lng"],
        "additionalProperties": false,
      };
    }
    if (draw(rng, 3) != 0) { required.push(name); }
  }
  { "type": "object", "properties": properties, "required": required, "additionalProperties": false }
}

function setup() -> Request {
  let rng = Rng { seed: 29 };
  let words = ["weather", "forecast", "city", "units", "latency", "budget", "invoice", "customer", "order", "status", "refund", "search", "query", "limit", "region", "timezone"];
  let input: InputItem[] = [];
  for (let i = 0; i < 24; i += 1) {
    let kind = draw(rng, 4);
    if (kind < 2) {
      let parts: InputPart[] = [];
      let count = 1 + draw(rng, 3);
      for (let p = 0; p < count; p += 1) {
        if (draw(rng, 4) == 0) {
          parts.push(InputImage {
            type: "input_image",
            image_url: "https://example.com/images/" + draw(rng, 100000).to_string() + ".png",
            file_id: null,
            detail: if (draw(rng, 2) == 0) { "auto" } else { null },
          });
        } else {
          parts.push(InputText { type: "input_text", text: sentence(rng, words, 12 + draw(rng, 30)) });
        }
      }
      input.push(MessageItem { role: if (kind == 0) { "user" } else { "assistant" }, content: parts });
    } else if (kind == 2) {
      let call = "call_" + draw(rng, 1000000).to_string();
      input.push(FunctionCallItem {
        type: "function_call",
        call_id: call,
        name: words[draw(rng, words.length())],
        arguments: "{\"query\":\"" + sentence(rng, words, 3) + "\",\"limit\":" + draw(rng, 50).to_string() + "}",
      });
    } else {
      input.push(FunctionCallOutputItem {
        type: "function_call_output",
        call_id: "call_" + draw(rng, 1000000).to_string(),
        output: sentence(rng, words, 20),
      });
    }
  }
  let tools: FunctionTool[] = [];
  for (let t = 0; t < 8; t += 1) {
    tools.push(FunctionTool {
      type: "function",
      name: "tool_" + words[draw(rng, words.length())] + "_" + t.to_string(),
      description: sentence(rng, words, 10),
      parameters: tool_schema(rng, words, 2 + draw(rng, 5)),
      strict: draw(rng, 2) == 0,
    });
  }
  Request {
    model: "gpt-5.1-mini",
    instructions: null,
    input: input,
    tools: tools,
    tool_choice: "auto",
    temperature: 0.7,
    top_p: null,
    max_output_tokens: 4096,
    parallel_tool_calls: true,
    reasoning: Reasoning { effort: "low", summary: null },
    metadata: { "session": "sess_" + draw(rng, 1000).to_string(), "trace": "tr_" + draw(rng, 100000).to_string() },
    store: false,
    user: null,
    stream: true,
  }
}

function run(request: Request) -> int {
  baml.json.stringify(request).length() * 3 + baml.json.stringify(request, omit_nulls = true).length()
}

function main() -> int {
  run(setup())
}
