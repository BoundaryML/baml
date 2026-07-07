// Cross-conversation long-term memory (scenario 21). Host persistence pending; the
// interface + negotiation are real and compiling.

/// One recalled memory with a relevance score.
class MemoryHit {
    text: string,
    score: float,
}

//baml:llm_capability
/// Provider-held long-term memory: remember facts, recall by query.
interface MemoryStore requires root.ai.Provider {
    function recall(self, query: string, k: int) -> MemoryHit[] throws root.errors.UnknownError
    function remember(self, fact: string) -> null throws root.errors.UnknownError
}
