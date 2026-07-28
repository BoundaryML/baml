function account_alias_tool() -> root.ai.Tool throws never {
    root.ai
        .tool(
            lookup_account,
            name = "search_knowledge",
            description = "Legacy account lookup alias. Call with customer_id.",
        )
}
