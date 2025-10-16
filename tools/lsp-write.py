#!/usr/bin/env -S uv run --script

# /// script
# dependencies = [
#   "bump2version",
#   "rich",
#   "termcolor",
#   "typer",
# ]
# ///


jsonrpc1 = """
Content-Length: 407

{"jsonrpc":"2.0","id":"27","method":"codeLens/resolve","params":{"range":{"start":{"line":36,"character":1053},"end":{"line":53,"character":1347}},"command":{"title":"▶ Test ExtractResume 💥","command":"baml.runBamlTest","arguments":[{"projectId":"/Users/sam/baml4/engine/baml-runtime/src/cli/initial_project/baml_src","testCaseName":"vaibhav_resume","functionName":"ExtractResume","showTests":true}]}}}
""".strip()


jsonrpc2 = """
Content-Length: 283

{"jsonrpc":"2.0","id":"28","method":"workspace/executeCommand","params":{"command":"baml.runBamlTest","arguments":[{"projectId":"/Users/sam/baml4/engine/baml-runtime/src/cli/initial_project/baml_src","testCaseName":"vaibhav_resume","functionName":"ExtractResume","showTests":true}]}}
""".strip()

template1 = """
{"jsonrpc":"2.0","id":"JSON_RPC_ID","method":"codeLens/resolve","params":{"range":{"start":{"line":36,"character":1053},"end":{"line":53,"character":1347}},"command":{"title":"▶ Test ExtractResume 💥","command":"baml.runBamlTest","arguments":[{"projectId":"/Users/sam/baml4/engine/baml-runtime/src/cli/initial_project/baml_src","testCaseName":"vaibhav_resume","functionName":"ExtractResume","showTests":true}]}}}
""".strip()

template2 = """
{"jsonrpc":"2.0","id":"JSON_RPC_ID","method":"workspace/executeCommand","params":{"command":"baml.runBamlTest","arguments":[{"projectId":"/Users/sam/baml4/engine/baml-runtime/src/cli/initial_project/baml_src","testCaseName":"vaibhav_resume","functionName":"ExtractResume","showTests":true}]}}
""".strip()


def write_jsonrpc(id: int, template: str):
    content = template.replace("JSON_RPC_ID", str(id))
    jsonrpc_str = f"""Content-Length: {len(bytes(content, "utf-8"))}

{content}"""
    return jsonrpc_str


if __name__ == "__main__":
    pipe = open("/tmp/lsp-fifo", "a")
    idx1 = 9
    idx2 = 10

    while True:
        pipe.write(write_jsonrpc(idx1, template1))
        pipe.write(write_jsonrpc(idx2, template2))

        idx1 += 2
        idx2 += 2

        input("Press Enter to continue...")
