// A seeded generator owns its state: a deep copy copies the state, so the copy
// draws the same stream as the original from then on, independently of it.

function random_seed() -> uint8array {
    "0123456789abcdef0123456789abcdef".to_utf8()
}

function xoshiro_copy_draws_independently() -> bool {
    let original = baml.random.Xoshiro256PlusPlus.new(seed = random_seed());
    let copy = baml.deep_copy(original);
    let first = original.random_int();
    let second = original.random_int();
    // The copy starts where the original was when copied, and drawing from
    // the original did not advance it.
    copy.random_int() == first && copy.random_int() == second && first != second
}

test "xoshiro_deep_copy_draws_independently" {
    assert.is_true(xoshiro_copy_draws_independently())
}

function chacha_copy_draws_independently() -> bool {
    let original = baml.random.ChaCha20.new(seed = random_seed());
    let copy = baml.deep_copy(original);
    let first = original.random_int();
    let second = original.random_int();
    copy.random_int() == first && copy.random_int() == second && first != second
}

test "chacha_deep_copy_draws_independently" {
    assert.is_true(chacha_copy_draws_independently())
}
