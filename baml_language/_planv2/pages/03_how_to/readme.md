# How-to guides

Each page in this section accomplishes one task with the public API.
The guides (`../02_guides/`) explain the system, and the reference
(`../04_reference/`) states signatures; a how-to page assumes both and
gets to the point. A page here may repeat a guide's statements when
the task needs them, and it never introduces behavior of its own.

- `01_retry_a_failed_parse_with_feedback.md` — re-ask the model with
  the parse failure in the transcript.
- `02_test_without_a_network.md` — drive an agent loop with scripted
  turns.
- `03_use_a_local_model.md` — point a function at an OpenAI-compatible
  endpoint.
- `04_observe_a_run_with_on_event.md` — react to journal events as
  they append.
- `05_attach_mcp_servers_to_claude_code.md` — attach MCP servers to
  the Claude Code CLI, at construction or mid-run.
- `06_use_mcp_tools_with_any_client.md` — turn an MCP server's tools
  into ordinary journaled tools that work with any client.
