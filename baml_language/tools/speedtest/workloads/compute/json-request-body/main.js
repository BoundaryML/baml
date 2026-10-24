class Rng {
  constructor(seed) { this.seed = seed; }
  draw(n) { this.seed = this.seed * 16807 % 2147483647; return this.seed % n; }
}

function sentence(rng, words, count) {
  const parts = [];
  for (let i = 0; i < count; i++) parts.push(words[rng.draw(words.length)]);
  return parts.join(" ");
}

function toolSchema(rng, words, fields) {
  const properties = {};
  const required = [];
  for (let f = 0; f < fields; f++) {
    const name = words[rng.draw(words.length)] + "_" + f;
    const kind = rng.draw(4);
    if (kind === 0) {
      properties[name] = { type: "string", description: sentence(rng, words, 6) };
    } else if (kind === 1) {
      properties[name] = { type: "integer", minimum: 0, maximum: 1000 + rng.draw(9000) };
    } else if (kind === 2) {
      properties[name] = { type: "array", items: { type: "string", enum: ["low", "medium", "high"] } };
    } else {
      properties[name] = {
        type: "object",
        properties: { lat: { type: "number" }, lng: { type: "number" } },
        required: ["lat", "lng"],
        additionalProperties: false,
      };
    }
    if (rng.draw(3) !== 0) required.push(name);
  }
  return { type: "object", properties, required, additionalProperties: false };
}

function setup() {
  const rng = new Rng(29);
  const words = ["weather", "forecast", "city", "units", "latency", "budget", "invoice", "customer", "order", "status", "refund", "search", "query", "limit", "region", "timezone"];
  const input = [];
  for (let i = 0; i < 24; i++) {
    const kind = rng.draw(4);
    if (kind < 2) {
      const parts = [];
      const count = 1 + rng.draw(3);
      for (let p = 0; p < count; p++) {
        if (rng.draw(4) === 0) {
          const url = "https://example.com/images/" + rng.draw(100000) + ".png";
          parts.push({ type: "input_image", image_url: url, file_id: null, detail: rng.draw(2) === 0 ? "auto" : null });
        } else {
          parts.push({ type: "input_text", text: sentence(rng, words, 12 + rng.draw(30)) });
        }
      }
      input.push({ role: kind === 0 ? "user" : "assistant", content: parts });
    } else if (kind === 2) {
      const call = "call_" + rng.draw(1000000);
      const name = words[rng.draw(words.length)];
      const query = sentence(rng, words, 3);
      input.push({ type: "function_call", call_id: call, name, arguments: "{\"query\":\"" + query + "\",\"limit\":" + rng.draw(50) + "}" });
    } else {
      const call = "call_" + rng.draw(1000000);
      input.push({ type: "function_call_output", call_id: call, output: sentence(rng, words, 20) });
    }
  }
  const tools = [];
  for (let t = 0; t < 8; t++) {
    const name = "tool_" + words[rng.draw(words.length)] + "_" + t;
    const description = sentence(rng, words, 10);
    const parameters = toolSchema(rng, words, 2 + rng.draw(5));
    tools.push({ type: "function", name, description, parameters, strict: rng.draw(2) === 0 });
  }
  const session = "sess_" + rng.draw(1000);
  const trace = "tr_" + rng.draw(100000);
  return {
    model: "gpt-5.1-mini",
    instructions: null,
    input,
    tools,
    tool_choice: "auto",
    temperature: 0.7,
    top_p: null,
    max_output_tokens: 4096,
    parallel_tool_calls: true,
    reasoning: { effort: "low", summary: null },
    metadata: { session, trace },
    store: false,
    user: null,
    stream: true,
  };
}

// `omit_nulls` leaves out an object field that holds null. The request's
// `json` values hold no nulls, so dropping every null property matches it.
const omitNulls = (key, value) => (value === null ? undefined : value);

function run(request) {
  return JSON.stringify(request).length * 3 + JSON.stringify(request, omitNulls).length;
}

function main() { return run(setup()); }
