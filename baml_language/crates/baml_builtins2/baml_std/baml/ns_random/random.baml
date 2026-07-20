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

    /// Returns a uniformly random `int`.
    ///
    /// Since BAML `int` is 63-bit, this may discard a single bit of entropy.
    /// This may be relevant when using deterministic PRNGs.
    function random_int(self) -> int throws never {
        let bytes = self.random(8);
        ((bytes[0] & 127) << 7) | (bytes[1] << 6) | (bytes[2] << 5) | (bytes[3] << 4) | (bytes[4] << 3) | (bytes[5] << 2) | (bytes[6] << 1) | bytes[7]
    }
}

/// A cryptographically secure generator backed by the operating system's
/// entropy source (a CSPRNG). Draws fresh entropy on every call and is not
/// seedable, so it is non-deterministic and suitable for security-sensitive
/// randomness such as keys, tokens, and nonces.
class SystemRandom {
    /// Returns a new `SystemRandom` instance using the global system random number generator.
    function get() -> SystemRandom throws never {
        SystemRandom {}
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
    _state: $rust_type

    /// Creates a new `Xoshiro256PlusPlus` pseudo-random number generator.
    ///
    /// ## Parameters
    /// - `seed`: The seed for the random number generator.
    ///   If no seed is provided, a random seed is generated using the system random number generator.
    ///   The seed must be at least 32 bytes long.
    ///
    /// ## Panics
    /// If `seed` is shorter than 32 bytes.
    //baml:mut_vm
    //baml:fallible
    function new(
        seed: uint8array = Rng.random(SystemRandom.get(), 32),
    ) -> Xoshiro256PlusPlus throws never {
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
    _state: $rust_type

    /// Creates a new `ChaCha20` pseudo-random number generator.
    ///
    /// ## Parameters
    /// - `seed`: The seed for the random number generator.
    ///   If no seed is provided, a random seed is generated using the system random number generator.
    ///   The seed must be at least 32 bytes long.
    ///
    /// ## Panics
    /// If `seed` is shorter than 32 bytes.
    //baml:mut_vm
    //baml:fallible
    function new(
        seed: uint8array = Rng.random(SystemRandom.get(), 32),
    ) -> ChaCha20 throws never {
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
