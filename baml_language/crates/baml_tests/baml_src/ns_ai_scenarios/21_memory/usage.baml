// scenario 21 — memory: ../../../../../llm-provider/ideas/scenarios/21-memory/README.md
// BLOCKED: P8 — durable stores; remember/recall shape against a scripted fake.

class ScriptedMemory {
    seed: string,

    implements baml.ai.Provider {}

    implements baml.ai.MemoryStore {
        function recall(self, query: string, k: int) -> baml.ai.MemoryHit[]
            throws baml.errors.UnknownError {
            [baml.ai.MemoryHit { text: self.seed + " (re: " + query + ")", score: 0.9 }]
        }
        function remember(self, fact: string) -> null
            throws baml.errors.UnknownError {
            null
        }
    }
}

test "21: recall surfaces scored hits for a query" {
    let p: baml.ai.Provider = ScriptedMemory { seed: "user likes turtles" };
    let none: baml.ai.MemoryHit[] = [];
    let hits: baml.ai.MemoryHit[] = match (p) {
        let m: baml.ai.MemoryStore => m.recall("pets", 3) catch (e) { _ => none },
        _ => none,
    };
    assert.equal(hits.length(), 1)
    assert.is_true(hits[0].score > 0.5)
    assert.is_true(hits[0].text.includes("turtles"))
}
