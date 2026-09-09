// A pure, splittable pseudo-random source.
//
// Generation must be a function of `(item, seed)` alone: the same seed
// reproduces the same case, child generators draw from independent streams,
// and adding a draw to one generator never reshuffles another. The stdlib's
// random generators carry hidden mutable state and cannot promise any of
// that, so this package bans them (see lint.sh).
//
// The core stream is xorshift32 (Marsaglia's 13/17/5 triple): a full-period
// permutation of the non-zero 32-bit integers. Every intermediate value here
// stays a non-negative integer below 2^48, so nothing can overflow BAML's
// 63-bit `int` and `>>` always behaves as a logical shift.

/// The state of a pure pseudo-random stream. Two seeds with the same `state`
/// produce the same draws forever.
///
/// Zero is xorshift's fixed point, so a zero state reads as the same stream
/// as state one; every `Seed` value denotes a valid stream.
class Seed {
    state: int,
}

/// One draw: the value and the seed that continues the stream.
class Draw {
    value: int,
    next: Seed,
}

/// Two independent streams derived from one seed.
class SeedPair {
    left: Seed,
    right: Seed,
}

/// `2^32 - 1`.
function mask32() -> int throws never {
    4294967295
}

/// Normalise any integer into a seed state.
function seed_from(state: int) -> Seed throws never {
    let s = state & mask32();
    Seed {
        state: if (s == 0) {
            1
        } else {
            s
        },
    }
}

/// Advance the stream once. The value is uniform over `[1, 2^32)`.
function next(seed: Seed) -> Draw throws never {
    let s0 = if (seed.state == 0) {
        1
    } else {
        seed.state & mask32()
    };
    let s1 = (s0 ^ (s0 << 13)) & mask32();
    let s2 = (s1 ^ (s1 >> 17)) & mask32();
    let s3 = (s2 ^ (s2 << 5)) & mask32();
    Draw { value: s3, next: Seed { state: s3 } }
}

/// Draw a value in `[0, n)` for `n > 0`. The modulo bias is below `n / 2^32`,
/// negligible for the pool sizes this tool draws from.
///
/// # Panics
///
/// - `baml.panics.DivisionByZero` if `n` is zero; callers pick from non-empty pools.
function below(seed: Seed, n: int) -> Draw throws never {
    let d = next(seed);
    Draw { value: d.value % n, next: d.next }
}

/// Fork the stream. `left` continues the parent; `right` is a fresh stream
/// keyed by the parent's state, so draws on one never affect the other.
function split(seed: Seed) -> SeedPair throws never {
    let d = next(seed);
    SeedPair { left: d.next, right: seed_from(mix32(d.value ^ 2654435769)) }
}

/// The seed for the `index`-th case of the item called `item_id`. Stable
/// across runs and across edits to other items.
function seed_for(item_id: string, index: int) -> Seed throws never {
    let id_hash = fnv1a32(item_id.to_utf8());
    seed_from(mix32(id_hash ^ mix32((index & mask32()) ^ 2654435769)))
}

/// `(a * b) mod 2^32` for `a` and `b` in `[0, 2^32)`, computed in two 16-bit
/// halves so no intermediate exceeds 2^48.
function mul32(a: int, b: int) -> int throws never {
    let lo = a & 65535;
    let hi = a >> 16;
    (((lo * b) & mask32()) + (((hi * b) & 65535) << 16)) & mask32()
}

/// MurmurHash3's 32-bit finaliser: a bijective avalanche mix.
function mix32(h: int) -> int throws never {
    let h0 = h & mask32();
    let h1 = h0 ^ (h0 >> 16);
    let h2 = mul32(h1, 2246822507);
    let h3 = h2 ^ (h2 >> 13);
    let h4 = mul32(h3, 3266489909);
    h4 ^ (h4 >> 16)
}

/// FNV-1a over `bytes`, 32-bit.
function fnv1a32(bytes: uint8array) -> int throws never {
    let h = 2166136261;
    for (let b in bytes.to_array()) {
        h = mul32(h ^ b, 16777619);
    }
    h
}
