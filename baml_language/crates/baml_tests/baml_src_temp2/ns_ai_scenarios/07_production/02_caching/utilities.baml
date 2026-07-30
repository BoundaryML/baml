function _managed_cache_policy_corpus() -> ai.MessageHistory throws never {
    ai.MessageHistory {
        entries: [
            ai
                .ChatMessage
                .user(
                    "Support policy: duplicate card charges are usually pending authorizations. "
                        .repeat(800),
                ),
        ],
    }
}
