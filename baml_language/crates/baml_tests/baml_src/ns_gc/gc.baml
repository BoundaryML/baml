// GC integration tests at the BAML language level.
//
// These exercise generational GC correctness through realistic programs: heavy
// allocation loops that generate many short-lived objects, and programs that
// mix allocation with data access. Loop bodies mutate locals (boxed in the
// test-block VM), so all logic lives in top-level helper functions with
// file-scoped unique names; results are compared with `assert.equal`.

// ── test_heavy_allocation_loop ────────────────────────────────────────────────
//
// Allocates ~1000 short-lived 3-element array objects across 1000 iterations to
// exercise Gen0 collection and ensure the accumulator `sum` is correctly
// preserved through every GC cycle.
//
// Expected: sum = 0*3 + 1*3 + ... + 999*3 = 3 * (999 * 1000 / 2) = 1498500

function gc_heavy_allocation_loop() -> int {
    let sum = 0;
    let i = 0;
    while (i < 1000) {
        let arr = [i, i * 2, i * 3];
        sum = sum + arr[2];
        i = i + 1;
    }
    sum
}

test "test_heavy_allocation_loop" {
    assert.equal(gc_heavy_allocation_loop(), 1498500)
}

// ── test_map_survives_gc_during_operations ────────────────────────────────────
//
// Builds a map, triggers allocation pressure with many short-lived arrays so
// the GC has chances to run and move objects, then reads the map's length to
// confirm no stale pointer corruption occurred in the map heap object.

function gc_build_map() -> map<string, int> {
    {
        "a": 1,
        "b": 4,
        "c": 9,
        "d": 16,
        "e": 25
    }
}

function gc_map_survives_pressure() -> int {
    let m = gc_build_map();

    // Trigger allocation pressure with many short-lived arrays so the GC has
    // chances to run and move objects, including the map.
    let i = 0;
    while (i < 500) {
        let tmp = [i, i + 1, i + 2];
        i = i + 1;
    }

    m.length()
}

test "test_map_survives_gc_during_operations" {
    assert.equal(gc_map_survives_pressure(), 5)
}
