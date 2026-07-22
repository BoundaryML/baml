test "guide 04.02: compaction creates a lossy conversation projection" {
    let compacted = compact_history(sample_history());
    assert.equal(compacted.entries.length(), 1);
    assert.contains(root.ai.message_text(compacted.entries[0]), "duplicate charge")
}

testset "integ-test-guide-04-compaction" {
    test "guide 04.02 live Anthropic: a compacted projection retains the required fact" {
        let source = sample_history();
        let compacted = compact_history(source);
        let task = ConversationProbe_task(compacted, live_anthropic());
        let response = task.run(
            runner = root.ai.run.GenerationWithMeta<ConversationProbe>.new(),
        );
        log.info(
            "live conversation compaction "
                + {
                    "source_messages": source.entries.length(),
                    "compacted_messages": compacted.entries.length(),
                    "evidence": response.value.evidence,
                }
                .to_string(),
        );
        assert.equal(source.entries.length(), 2);
        assert.equal(compacted.entries.length(), 1);
        assert.contains(response.value.evidence.to_lower_case(), "duplicate")
    }
}
