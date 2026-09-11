class HeapStatsAllocation {
    value: int,
}

testset "heap_stats" {
    test "reports the calling runtime and serializes" {
        let stats = baml.sys.heap_stats();
        assert.is_true(stats.compile_time_objects > 0);
        assert.is_true(stats.runtime_objects >= 0);
        assert.is_true(stats.tlab_chunks >= 0);
        assert.is_true(stats.active_handles >= 0);
        assert.equal(stats.total_objects, stats.compile_time_objects + stats.runtime_objects);
        let decoded = baml.json.from_string<baml.sys.HeapStats>(baml.json.to_string(stats));
        assert.equal(decoded, stats);
    }

    test "reflects allocations in the calling VM" {
        let before = baml.sys.heap_stats();
        let values: HeapStatsAllocation[] = [];
        for (let i = 0; i < 4096; i += 1) {
            values.push(HeapStatsAllocation { value: i });
        }
        let after = baml.sys.heap_stats();
        assert.is_true(after.runtime_objects > before.runtime_objects);
        assert.is_true(after.tlab_chunks > before.tlab_chunks);
        assert.equal(after.compile_time_objects, before.compile_time_objects);
        assert.equal(values[4095].value, 4095);
    }
}
