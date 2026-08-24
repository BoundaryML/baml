// Tests for the `bool` builtin class.
//
// `bool.random` is the only member, so these also pin that the new compiler's
// primitive static-qualifier path resolves `bool` and its companion class.

function bool_random_seed() -> uint8array {
    // Bound to a local: a byte-string literal in tail position is not
    // recognized as the return value (E0029), like the workaround in ns_ints.
    let seed = b"\x61\x62\x63\x64\x65\x66\x67\x68\x69\x6a\x6b\x6c\x6d\x6e\x6f\x70\x71\x72\x73\x74\x75\x76\x77\x78\x79\x7a\x7b\x7c\x7d\x7e\x7f\x80";
    seed
}

// Both outcomes must appear, at a rate near half. Over 1000 draws each count is
// Binomial(1000, 0.5); P(count <= 400) is on the order of 1e-10.
function bool_random_covers_both_values() -> bool {
    let rng = baml.random.Xoshiro256PlusPlus.new(seed = bool_random_seed());
    let t = 0;
    let f = 0;
    let i = 0;
    while (i < 1000) {
        if (bool.random(rng = rng)) {
            t = t + 1
        } else {
            f = f + 1
        }
        i = i + 1
    }
    t > 400 && f > 400 && (t + f) == 1000
}

test "bool_random_covers_both_values" {
    assert.is_true(bool_random_covers_both_values())
}

// Two generators on the same seed must agree draw for draw.
function bool_random_seeded_is_reproducible() -> bool {
    let a = baml.random.Xoshiro256PlusPlus.new(seed = bool_random_seed());
    let b = baml.random.ChaCha20.new(seed = bool_random_seed());
    let c = baml.random.ChaCha20.new(seed = bool_random_seed());
    let ok = true;
    let i = 0;
    while (i < 100) {
        if (bool.random(rng = b) != bool.random(rng = c)) {
            ok = false
        } else {
            null
        }
        // `a` is drawn from too, so a shared-state bug between generators would
        // desynchronize `b` and `c`.
        bool.random(rng = a);
        i = i + 1
    }
    ok
}

test "bool_random_seeded_is_reproducible" {
    assert.is_true(bool_random_seeded_is_reproducible())
}

// The default `rng` argument resolves without an explicit generator.
function bool_random_default_rng_runs() -> bool {
    let r = bool.random();
    r || !r
}

test "bool_random_default_rng_runs" {
    assert.is_true(bool_random_default_rng_runs())
}
