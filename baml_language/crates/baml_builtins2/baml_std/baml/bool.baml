/// A boolean value — either `true` or `false`.
class Bool {
    /// Returns a random boolean value.
    ///
    /// Optionally accepts a custom random number generator and will consume one byte of entropy from it.
    function random(rng: root.random.Rng = root.random.SystemRandom.get()) -> bool throws never {
        (rng.random(1)[0] & 128) == 128
    }
}
