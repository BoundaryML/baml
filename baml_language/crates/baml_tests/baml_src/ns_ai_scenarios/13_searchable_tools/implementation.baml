class S13CatalogEntry {
  name: string,
  description: string,
}

class S13SearchArgs {
  query: string,
}

class S13LookupOrderArgs {
  order_id: string,
}

function s13_catalog() -> S13CatalogEntry[] throws never {
  [
    S13CatalogEntry { name: "lookup_order", description: "Look up shipping status." },
    S13CatalogEntry { name: "process_refund", description: "Issue an approved refund." },
  ]
}

function s13_search_tool() -> root.ai.Tool throws never {
  root.ai.Tool.typed<S13SearchArgs>("tool_search", "Search the tool catalog.")
}

function s13_lookup_tool() -> root.ai.Tool throws never {
  root.ai.Tool.typed<S13LookupOrderArgs>("lookup_order", "Look up an order by id.")
}

function s13_dispatch(
  loaded: string[],
) -> (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never {
  (calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] {
    calls.map((call: root.ai.ToolCall) -> root.ai.ToolResult {
      if (call.name == "tool_search") {
        loaded.push("lookup_order");
        return root.ai.ToolResult {
          id: call.id,
          output: baml.json.to_string(s13_catalog()) catch (e) { _ => "[]" },
        };
      }
      root.ai.invoke_tool<S13LookupOrderArgs>(call, (args: S13LookupOrderArgs) -> string {
        `order ${args.order_id} arrives Tuesday`
      })
    })
  }
}
