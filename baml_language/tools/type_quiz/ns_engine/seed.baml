// A pure, splittable pseudo-random source.
//
// Generation must be a function of `(item, seed)` alone: the same seed
// reproduces the same case, child generators draw from decorrelated streams,
// and adding a draw to one generator never reshuffles another. The stdlib's
// random generators are stateful objects rather than values and cannot
// promise that, so this package bans them (see lint.sh).
//
// The core stream is xorshift32 (Marsaglia's 13/17/5 triple): a full-period
// permutation of the non-zero 32-bit integers. Every operand is masked to a
// non-negative value below 2^48 before it is shifted or multiplied, so
// nothing can overflow BAML's 63-bit `int` (`+` and `*` panic on overflow,
// `<<` wraps silently), and arithmetic and logical right shift coincide.

/// The state of a pure pseudo-random stream. Two seeds with the same `state`
/// produce the same draws forever.
///
/// Every `int` denotes a stream: the state is read modulo 2^32, and zero,
/// xorshift's fixed point, reads as state one.
class Seed {
    state: int,
}

/// One draw: the value and the seed that continues the stream.
class Draw {
    value: int,
    next: Seed,
}

/// A value drawn from a stream and the seed that continues it. The one shape
/// every seeded generator returns.
class Rolled<T> {
    value: T,
    next: Seed,
}

/// Two streams derived from one seed: `left` continues the parent, `right`
/// is keyed by the parent's state through an avalanche mix. They are
/// decorrelated offsets into xorshift32's single cycle, not provably
/// disjoint substreams.
class SeedPair {
    left: Seed,
    right: Seed,
}

/// `2^32 - 1`.
function mask32() -> int throws never {
    4294967295
}

/// The stream state for any integer: masked, mixed so that neighbouring
/// integers start far apart, and moved off the zero fixed point.
function seed_from(state: int) -> Seed throws never {
    Seed { state: normalise(mix32(state & mask32())) }
}

/// A non-zero 32-bit state.
function normalise(state: int) -> int throws never {
    let s = state & mask32();
    if (s == 0) {
        1
    } else {
        s
    }
}

/// Advance the stream once. The value is uniform over `[1, 2^32)`.
function next(seed: Seed) -> Draw throws never {
    let s0 = normalise(seed.state);
    let s1 = (s0 ^ (s0 << 13)) & mask32();
    let s2 = (s1 ^ (s1 >> 17)) & mask32();
    let s3 = (s2 ^ (s2 << 5)) & mask32();
    Draw { value: s3, next: Seed { state: s3 } }
}

/// Draw a value in `[0, n)` for `n > 0`. Each residue's probability differs
/// from `1/n` by less than `2^-32`, negligible for the pool sizes this tool
/// draws from.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if `n` is not positive: every caller draws
///   from a non-empty pool.
function below(seed: Seed, n: int) -> Draw throws never {
    if (n <= 0) {
        throw baml.panics.Unreachable { message: `below() needs a positive bound, got ${n}` }
    }
    let d = next(seed);
    Draw { value: d.value % n, next: d.next }
}

/// Fork the stream.
function split(seed: Seed) -> SeedPair throws never {
    let d = next(seed);
    SeedPair { left: d.next, right: Seed { state: normalise(mix32(d.value ^ 2654435769)) } }
}

/// The seed for the `index`-th case of the item called `item_id`. Stable
/// across runs and across edits to other items.
function seed_for(item_id: string, index: int) -> Seed throws never {
    let id_hash = fnv1a32(item_id.to_utf8());
    Seed { state: normalise(mix32(id_hash ^ mix32((index & mask32()) ^ 2654435769))) }
}

/// `n` distinct elements of `pool`, in draw order, and the seed that
/// continues the stream. Yields fewer only when the pool holds fewer than
/// `n`; callers that need exactly `n` are responsible for the pool size.
function pick_distinct<T>(seed: Seed, pool: T[], n: int) -> Rolled<T[]> throws never {
    let remaining = pool.slice(0, pool.length());
    let taken: T[] = [];
    let s = seed;
    while (taken.length() < n && remaining.length() > 0) {
        let d = below(s, remaining.length());
        s = d.next;
        match (remaining.remove_at(d.value)) {
            let v: T => taken.push(v),
            null => {
                throw baml.panics.Unreachable { message: "remove_at inside its own bounds returned null" }
            },
        };
    }
    Rolled<T[]> { value: taken, next: s }
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
    bytes.to_array().reduce(
        (h, b) -> {
            mul32(h ^ b, 16777619)
        },
        2166136261,
    )
}
