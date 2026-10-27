import json


class Rng:
    def __init__(self, seed):
        self.seed = seed

    def draw(self, n):
        self.seed = self.seed * 16807 % 2147483647
        return self.seed % n


def setup():
    rng = Rng(23)
    words = ["agent", "tool", "result", "latency", "token", "stream", "retry", "model", "prompt", "cache", "session", "vector"]
    types = ["message", "tool_call", "tool_result", "status"]
    roles = ["user", "assistant", "system"]
    tools = ["search", "calculator", "browser", "shell"]
    lines = []
    for i in range(2000):
        kind = rng.draw(4)
        parts = []
        length = 0
        while length < 290:
            word = words[rng.draw(12)]
            parts.append(word)
            length += len(word) + 1
        content = "\"" + " ".join(parts) + "\""
        if rng.draw(3) == 0:
            content = "\"" + " ".join(parts) + "\\n\\\"done\\\"\""
        if kind == 3:
            content = "null"
        role = "\"" + roles[rng.draw(3)] + "\"" if kind == 0 else "null"
        line = "{\"id\":\"evt_" + str(i) + "_" + str(rng.draw(1000000)) + "\""
        line += ",\"session_id\":\"sess_" + str(rng.draw(50)) + "\""
        line += ",\"event_type\":\"" + types[kind] + "\""
        line += ",\"role\":" + role
        line += ",\"content\":" + content
        if kind == 1 or kind == 2:
            line += ",\"tool_name\":\"" + tools[rng.draw(4)] + "\""
        line += ",\"extra\":{\"a\":[1,2,3],\"b\":\"z\"}"
        line += ",\"created_at\":" + str(1700000000000 + i * 1000 + rng.draw(1000)) + "}"
        lines.append(line)
    return lines


def run(lines):
    check = 0
    for line in lines:
        e = json.loads(line)
        h = e["created_at"] % 100000 + len(e["id"]) + len(e["session_id"]) + len(e["event_type"])
        role = e["role"] if e["role"] is not None else "x"
        content = e["content"] if e["content"] is not None else "nu"
        tool_name = e.get("tool_name")
        h += len(role) + len(content) + len(tool_name if tool_name is not None else "abc")
        check = (check + h) % 1000000007
    return check


def main():
    return run(setup())
