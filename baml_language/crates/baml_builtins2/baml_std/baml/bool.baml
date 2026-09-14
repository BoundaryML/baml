/// A boolean value — either `true` or `false`.
class Bool {
    /// Returns a random `true` or `false`, drawn from `rng`.
    ///
    /// `rng` defaults to `random.SystemRandom`, the host's cryptographic
    /// entropy source; pass a seeded generator to make the draw reproducible.
    /// Each call consumes one byte of entropy and tests its high bit.
    function random(rng: root.random.Rng = root.random.SystemRandom.get()) -> bool throws never {
        (rng.random(1)[0] & 128) == 128
    }
}
