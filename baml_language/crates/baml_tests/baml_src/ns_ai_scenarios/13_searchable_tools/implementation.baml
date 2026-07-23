class S13CatalogEntry {
  name: string,
  description: string,
  input_schema: json,
}

class S13SearchArgs {
  query: string,
}

class S13LookupOrderArgs {
  order_id: string,
}

function s13_catalog() -> S13CatalogEntry[] throws never {
  [
    S13CatalogEntry {
      name: "mcp__orders__lookup_order",
      description: "Look up shipping status.",
      input_schema: {
        "type": "object",
        "properties": { "order_id": { "type": "string" } },
        "required": ["order_id"],
      },
    },
  ]
}

function s13_search_tool() -> root.ai.Tool throws never {
  root.ai.Tool.typed<S13SearchArgs>("tool_search", "Search the tool catalog.")
}

function s13_tool_from_mcp(entry: S13CatalogEntry) -> root.ai.Tool throws never {
  root.ai.Tool.from_json_schema(entry.name, entry.description, entry.input_schema)
}

function s13_dispatch(
  events: string[],
  registry: root.ai.ToolRegistry,
) -> (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never {
  (calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] {
    calls.map((call: root.ai.ToolCall) -> root.ai.ToolResult {
      if (call.name == "tool_search") {
        let catalog = s13_catalog();
        for (let entry in catalog) { registry.add(s13_tool_from_mcp(entry)); }
        events.push("loaded:mcp__orders__lookup_order");
        return root.ai.ToolResult {
          id: call.id,
          output: baml.json.to_string(s13_catalog()) catch (e) { _ => "[]" },
        };
      }
      if (call.name == "mcp__orders__lookup_order") {
        events.push("called:mcp__orders__lookup_order");
      }
      root.ai.invoke_tool<S13LookupOrderArgs>(call, (args: S13LookupOrderArgs) -> string {
        `order ${args.order_id} arrives Tuesday`
      })
    })
  }
}
