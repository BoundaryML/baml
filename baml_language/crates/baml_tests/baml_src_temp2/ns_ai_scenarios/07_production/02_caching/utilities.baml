function _managed_cache_policy_corpus() -> root.ai.MessageHistory throws never {
    root.ai.MessageHistory {
        entries: [
            root.ai
                .ChatMessage
                .user(
                    "Support policy: duplicate card charges are usually pending authorizations. "
                        .repeat(800),
                ),
        ],
    }
}
