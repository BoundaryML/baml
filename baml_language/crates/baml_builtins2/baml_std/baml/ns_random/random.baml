/// A source of random bytes.
///
/// Three implementations are provided by the standard library:
/// - `SystemRandom` is a secure singleton provider that calls out to the system's entropy source
/// - `Xoshiro256PlusPlus` is a fast seedable PRNG
/// - `ChaCha20` is a secure seedable PRNG
interface Rng {
    /// Produces `bytes` uniformly random bytes.
    ///
    /// ## Panics
    ///
    /// - If `bytes` is negative.
    /// - If `bytes` bytes cannot be allocated (`baml.panics.AllocFailure`).
    /// - If the implementation fails to produce the requested number of bytes
    ///   (for example, if the underlying system random number generator is unavailable)
    function random(self, bytes: int) -> uint8array throws never

    /// Returns a uniformly random `int` over the full `[int.min_value(),
    /// int.max_value()]` range.
    ///
    /// BAML `int` is 63-bit, so this discards a single bit of the 64 drawn (the
    /// top bit of the first byte). The remaining 63 bits give a uniform value:
    /// the low 62 bits form the magnitude and the 63rd is the sign.
    function random_int(self) -> int throws never {
        let bytes = self.random(8);
        // A uniform 62-bit magnitude in `[0, int.max_value()]`. The first byte
        // is masked to 6 bits (`& 63`) so its top contribution lands at bit 61,
        // never bit 62 — the i63 sign bit — which would overflow.
        let magnitude = ((bytes[0] & 63) << 56) | (bytes[1] << 48) | (bytes[2] << 40) | (bytes[3] << 32) | (bytes[4] << 24) | (bytes[5] << 16) | (bytes[6] << 8) | bytes[7];
        // Use bit 6 of the first byte as a sign bit. `0 - magnitude - 1` maps
        // `[0, 2^62)` onto `[int.min_value(), -1]`, so the two branches together
        // tile the whole signed range with no overlap or overflow.
        if ((bytes[0] & 64) == 0) {
            magnitude
        } else {
            0 - magnitude - 1
        }
    }
}

/// A cryptographically secure generator backed by the operating system's
/// entropy source (a CSPRNG). Draws fresh entropy on every call and is not
/// seedable, so it is non-deterministic and suitable for security-sensitive
/// randomness such as keys, tokens, and nonces.
class SystemRandom {
    /// Returns a new `SystemRandom` instance using the global system random number generator.
    function get() -> SystemRandom throws never {
        SystemRandom {  }
    }

    implements Rng {
        function random(self, bytes: int) -> uint8array throws never {
            $rust_io_function
        }
        function random_int(self) -> int throws never {
            $rust_io_function
        }
    }
}

/// A fast, seedable, deterministic PRNG (xoshiro256++). Given the same seed it
/// always reproduces the same stream, which makes it ideal for tests,
/// simulations, and sampling.
///
/// NOT cryptographically secure: its output is predictable from observed draws.
/// Never use it for keys, tokens, nonces, or any security-sensitive value — use
/// `SystemRandom` or a well-seeded `ChaCha20` instead.
class Xoshiro256PlusPlus {
    /// Opaque, mutex-guarded generator state owned by the Rust runtime.
    _state: $rust_type,

    /// Creates a new `Xoshiro256PlusPlus` pseudo-random number generator.
    ///
    /// ## Parameters
    /// - `seed`: The seed for the random number generator.
    ///   If no seed is provided, a random seed is generated using the system random number generator.
    ///   The seed must be at least 32 bytes long.
    ///
    /// ## Panics
    /// If `seed` is shorter than 32 bytes.
    function new(
        seed: uint8array = SystemRandom.get().random(32),
    ) -> Xoshiro256PlusPlus throws never {
        // A `$rust_function` builtin can't fill omitted default args, so this
        // thin wrapper resolves the default seed and forwards to `_new` (whose
        // `seed` is required) — the `net.TcpStream.connect`/`_connect` convention.
        Xoshiro256PlusPlus._new(seed)
    }

    //baml:mut_vm
    //baml:fallible
    function _new(seed: uint8array) -> Xoshiro256PlusPlus throws never {
        $rust_function
    }

    implements Rng {
        //baml:vm
        //baml:fallible
        function random(self, bytes: int) -> uint8array throws never {
            $rust_function
        }
        //baml:vm
        function random_int(self) -> int throws never {
            $rust_function
        }
    }
}

/// A seedable, deterministic CSPRNG (ChaCha20). Given the same seed it always
/// reproduces the same stream. It is cryptographically secure *only* when seeded
/// with high-entropy input — e.g. 32 bytes drawn from `SystemRandom` (the
/// default when no seed is given). A low-entropy or attacker-known seed makes
/// its output predictable.
class ChaCha20 {
    /// Opaque, mutex-guarded generator state owned by the Rust runtime.
    _state: $rust_type,

    /// Creates a new `ChaCha20` pseudo-random number generator.
    ///
    /// ## Parameters
    /// - `seed`: The seed for the random number generator.
    ///   If no seed is provided, a random seed is generated using the system random number generator.
    ///   The seed must be at least 32 bytes long.
    ///
    /// ## Panics
    /// If `seed` is shorter than 32 bytes.
    function new(seed: uint8array = SystemRandom.get().random(32)) -> ChaCha20 throws never {
        // A `$rust_function` builtin can't fill omitted default args, so this
        // thin wrapper resolves the default seed and forwards to `_new` (whose
        // `seed` is required) — the `net.TcpStream.connect`/`_connect` convention.
        ChaCha20._new(seed)
    }

    //baml:mut_vm
    //baml:fallible
    function _new(seed: uint8array) -> ChaCha20 throws never {
        $rust_function
    }

    implements Rng {
        //baml:vm
        //baml:fallible
        function random(self, bytes: int) -> uint8array throws never {
            $rust_function
        }
        //baml:vm
        function random_int(self) -> int throws never {
            $rust_function
        }
    }
}
