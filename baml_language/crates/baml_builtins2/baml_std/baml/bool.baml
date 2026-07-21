/// A boolean value — either `true` or `false`.
class Bool {
    function random(rng: root.random.Rng = root.random.SystemRandom.get()) -> bool throws never {
        (rng.random(1)[0] & 128) == 128
    }
}
