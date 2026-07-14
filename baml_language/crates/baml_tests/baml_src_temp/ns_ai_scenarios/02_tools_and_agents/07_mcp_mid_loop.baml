test "guide 02.07: an MCP schema can become a runtime tool" {
  let mcp_tool = root.ai.Tool.from_json_schema(
    "mcp__orders__lookup_order",
    "Look up an order discovered from MCP.",
    {
      "type": "object",
      "properties": { "order_id": { "type": "string" } },
      "required": ["order_id"],
    },
  );
  let registry = root.ai.ToolRegistry.new([knowledge_tool()]);
  let hooks = no_op_hooks();
  hooks.registry = registry;
  hooks.tool_to_add = mcp_tool;
  let options = guide_agent_options([], guide_dispatch);
  options.tool_registry = registry;
  options.hooks = hooks;
  root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), []),
    options,
  );
  assert.equal(registry.snapshot().length(), 2)
}
