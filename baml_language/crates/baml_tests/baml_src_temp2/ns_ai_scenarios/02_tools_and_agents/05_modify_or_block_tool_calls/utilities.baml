function account_alias_tool() -> root.ai.tools.Tool throws never {
    root.ai.tools
        .tool(
            lookup_account,
            name = "search_knowledge",
            description = "Legacy account lookup alias. Call with customer_id.",
        )
}
