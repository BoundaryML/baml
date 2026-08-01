# MCP

## MCP servers are toolboxes

An MCP server is a source of tools discovered at runtime. Connecting one
yields ordinary `Tool` values:

```baml
let gh = baml.mcp.connect("https://mcp.github.dev", auth = env.GITHUB_TOKEN);
let tools = gh.tools();          // Tool[] — names, schemas, descriptions from the server
```

MCP tools and local tools are the same type. Mix them in one toolbox:

```baml
let s = TriageAgent@session(
    repo = "boundaryml/baml",
    tools = [tool(read_file), tool(run_bash)].concat(gh.tools()),
);
```

## Dynamic discovery

MCP tool lists can change while a session runs. Changes are applied
through the policy (`MountTools` / `UnmountTools`) and recorded as
`ToolsChanged` events, like every other capability change. The journal
shows which tools were available at every point in the conversation —
including tools that came from a server that no longer exists.

## Schemas and validation

Local tools get schemas from reflection; MCP tools carry schemas from the
server. Validation is the same in both cases: bad arguments are rejected
before the call and returned to the model as an error result.

## Replay caveat

MCP tool results are external effects. A replayed journal returns the
recorded results without contacting the server; a *resumed* session calls
the live server again. Name stability matters: if a server renames a
tool, journals referencing the old name still render, but the model can
no longer call it.
