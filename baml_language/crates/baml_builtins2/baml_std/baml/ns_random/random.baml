interface Rng {
    /// Produces `bytes` uniformly random bytes.
    ///
    /// ## Panics
    ///
    /// - If `bytes` is negative.
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
