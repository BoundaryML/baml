function account_alias_tool() -> ai.tools.Tool throws never {
    ai
        .tools
        .tool(
            lookup_account,
            name = "search_knowledge",
            description = "Legacy account lookup alias. Call with customer_id.",
        )
}
