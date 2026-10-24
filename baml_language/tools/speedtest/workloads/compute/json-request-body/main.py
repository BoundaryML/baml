import json


class Rng:
    def __init__(self, seed):
        self.seed = seed

    def draw(self, n):
        self.seed = self.seed * 16807 % 2147483647
        return self.seed % n


def sentence(rng, words, count):
    return " ".join(words[rng.draw(len(words))] for _ in range(count))


def tool_schema(rng, words, fields):
    properties = {}
    required = []
    for f in range(fields):
        name = words[rng.draw(len(words))] + "_" + str(f)
        kind = rng.draw(4)
        if kind == 0:
            properties[name] = {"type": "string", "description": sentence(rng, words, 6)}
        elif kind == 1:
            properties[name] = {"type": "integer", "minimum": 0, "maximum": 1000 + rng.draw(9000)}
        elif kind == 2:
            properties[name] = {"type": "array", "items": {"type": "string", "enum": ["low", "medium", "high"]}}
        else:
            properties[name] = {
                "type": "object",
                "properties": {"lat": {"type": "number"}, "lng": {"type": "number"}},
                "required": ["lat", "lng"],
                "additionalProperties": False,
            }
        if rng.draw(3) != 0:
            required.append(name)
    return {"type": "object", "properties": properties, "required": required, "additionalProperties": False}


def setup():
    rng = Rng(29)
    words = ["weather", "forecast", "city", "units", "latency", "budget", "invoice", "customer", "order", "status", "refund", "search", "query", "limit", "region", "timezone"]
    items = []
    for _ in range(24):
        kind = rng.draw(4)
        if kind < 2:
            parts = []
            for _ in range(1 + rng.draw(3)):
                if rng.draw(4) == 0:
                    url = "https://example.com/images/" + str(rng.draw(100000)) + ".png"
                    detail = "auto" if rng.draw(2) == 0 else None
                    parts.append({"type": "input_image", "image_url": url, "file_id": None, "detail": detail})
                else:
                    parts.append({"type": "input_text", "text": sentence(rng, words, 12 + rng.draw(30))})
            items.append({"role": "user" if kind == 0 else "assistant", "content": parts})
        elif kind == 2:
            call = "call_" + str(rng.draw(1000000))
            name = words[rng.draw(len(words))]
            query = sentence(rng, words, 3)
            arguments = "{\"query\":\"" + query + "\",\"limit\":" + str(rng.draw(50)) + "}"
            items.append({"type": "function_call", "call_id": call, "name": name, "arguments": arguments})
        else:
            call = "call_" + str(rng.draw(1000000))
            items.append({"type": "function_call_output", "call_id": call, "output": sentence(rng, words, 20)})
    tools = []
    for t in range(8):
        name = "tool_" + words[rng.draw(len(words))] + "_" + str(t)
        description = sentence(rng, words, 10)
        parameters = tool_schema(rng, words, 2 + rng.draw(5))
        tools.append({"type": "function", "name": name, "description": description, "parameters": parameters, "strict": rng.draw(2) == 0})
    session = "sess_" + str(rng.draw(1000))
    trace = "tr_" + str(rng.draw(100000))
    return {
        "model": "gpt-5.1-mini",
        "instructions": None,
        "input": items,
        "tools": tools,
        "tool_choice": "auto",
        "temperature": 0.7,
        "top_p": None,
        "max_output_tokens": 4096,
        "parallel_tool_calls": True,
        "reasoning": {"effort": "low", "summary": None},
        "metadata": {"session": session, "trace": trace},
        "store": False,
        "user": None,
        "stream": True,
    }


def omit_nulls(value):
    # `omit_nulls` leaves out an object field that holds null. The request's
    # `json` values hold no nulls, so dropping every null key matches it.
    if isinstance(value, dict):
        return {k: omit_nulls(v) for k, v in value.items() if v is not None}
    if isinstance(value, list):
        return [omit_nulls(v) for v in value]
    return value


def run(request):
    compact = json.dumps(request, separators=(",", ":"))
    pruned = json.dumps(omit_nulls(request), separators=(",", ":"))
    return len(compact) * 3 + len(pruned)


def main():
    return run(setup())
