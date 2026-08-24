use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use baml_type::Int63;
use borsh::{BorshDeserialize, BorshSerialize};

use crate::{HeapPtr, Object};

/// Runtime values — a single tagged 64-bit word.
///
/// This is a packed tagged-pointer representation. The low bit (or low
/// 3 bits, depending on the category) carries a type tag; the upper
/// bits carry the payload. Hardware-atomic on aligned 8-byte stores
/// across all supported targets (x86-64, ARM64, `ARMv7`, RISC-V).
///
/// # Encoding (low 3 bits = tag category)
///
/// | Bit pattern                            | Meaning                                          |
/// | -------------------------------------- | ------------------------------------------------ |
/// | `0x0000_0000_0000_0000`                | `Null` — the only zero pointer                   |
/// | `0x0000_0000_0000_0002`                | `Bool(false)` sentinel                           |
/// | `0x0000_0000_0000_0004`                | `Bool(true)` sentinel                            |
/// | `0x0000_0000_0000_0006`                | `OmittedArg` sentinel                            |
/// | `xxxxx...xxx1` (low bit set)           | `Int(i63)` — sign-extend via `(v as i64) >> 1`   |
/// | `0xxxxx...xxx0` (low 3 bits zero, ≠0)  | `Object(HeapPtr)` — heap pointer (8-byte align)  |
///
/// # On `Float`
///
/// `Float(f64)` does NOT have an inline encoding. Floats are heap-boxed
/// as `Object::Float(f64)` and referenced via the pointer arm. This
/// trades float-arithmetic cost (one heap alloc per result) for a
/// uniform 8-byte `Value` representation, which is what makes every
/// read/write hardware-atomic and gives ~50% cache footprint
/// reduction. BAML programs are integer-and-object-heavy so the
/// trade-off favors us.
///
/// # On range loss
///
/// Integers shrink from i64 to i63 (max ~4.6e18). Holds nanosecond
/// timestamps until year ~2200 with margin. For larger integers,
/// callers must allocate a heap-boxed integer (not yet implemented).
///
/// # `PartialEq` / `Hash`
///
/// Derived on the underlying u64. This gives bit-equality, which is
/// reference equality for heap-allocated objects (including boxed
/// floats — two `Value::float(3.14, ...)` calls produce different
/// pointers and compare unequal). Content equality for strings,
/// arrays, etc. is handled at the user-visible `==` operator in the
/// VM dispatch, not here.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Value(u64);

/// Categorical view of a `Value` for pattern matching.
///
/// Returned by [`Value::kind`]. The optimizer typically inlines the
/// match and folds the discrimination into a tight switch table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ValueKind {
    Null,
    OmittedArg,
    Int(i64),
    Bool(bool),
    Object(HeapPtr),
}

// The tagged-pointer encoding *is* a hot path: every Value access
// goes through `as_int`/`as_object_ptr`/`kind`, and the encoding round-trips
// between `u64` and signed `i64` by design (shift-left/right with sign
// extension is what gives us i63 ints in a u64). The explicit `bits & 0b111`
// checks for "low 3 bits clear" are more idiomatic for tagged pointers than
// the suggested `.trailing_zeros() >= 3`.
#[allow(
    clippy::inline_always,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::verbose_bit_mask
)]
impl Value {
    // ── Singletons ────────────────────────────────────────────────────────
    pub const NULL: Value = Value(0);
    pub const FALSE: Value = Value(2);
    pub const TRUE: Value = Value(4);
    pub const OMITTED_ARG: Value = Value(6);

    /// Largest representable BAML integer (`2^62 - 1 = 4_611_686_018_427_387_903`).
    ///
    /// Integers in BAML are i63 (low bit reserved for the tag). Values
    /// outside `[INT_MIN, INT_MAX]` cannot round-trip through `Value::int`.
    pub const INT_MAX: i64 = Int63::MAX.get();

    /// Smallest representable BAML integer (`-2^62 = -4_611_686_018_427_387_904`).
    pub const INT_MIN: i64 = Int63::MIN.get();

    // ── Tagged-int fast-path arithmetic ───────────────────────────────────
    //
    // Both operands' bit patterns are `(real << 1) | 1` (low bit = tag).
    // We can do arithmetic directly on the tagged bits without the
    // shift-right / shift-left / or sequence that goes through `as_int`
    // and `Value::int`. The tag bit is preserved by these tricks.
    //
    // Add: (ra<<1|1) + (rb<<1|1) - 1 = ((ra+rb)<<1) | 1
    // Sub: (ra<<1|1) - (rb<<1|1) + 1 = ((ra-rb)<<1) | 1
    //
    // Wrapping is correct: i63 ranges produce results that fit in i63
    // (modulo wrap, same as the previous `l + r` on i64s).
    //
    // For comparison, `(ra<<1)|1` < `(rb<<1)|1` iff `ra < rb` (shift-left
    // preserves signed ordering; the tag bit is the same in both so it
    // doesn't affect the comparison). Bits interpreted as i64 yield the
    // signed ordering of the underlying i63 values.

    /// Sum of two `Int`-tagged Values, or `None` on i63 overflow — computed
    /// without untagging.
    ///
    /// When present, the result is the exact tagged sum. The overflow test is
    /// exact and nearly free: in the tag encoding a value `x`
    /// is stored as `(x << 1) | 1`, so `a + (b - 1)` (the tagged sum, as a
    /// *signed* i64) overflows i64 precisely when `x + y` leaves the i63 range
    /// `[INT_MIN, INT_MAX]`. So the hardware signed-overflow flag of one add is
    /// the i63 range check — no shift-out/range-compare/re-encode needed.
    ///
    /// Both inputs must be `Int`-tagged; debug builds assert this contract.
    #[inline(always)]
    pub const fn tagged_int_add_checked(a: Value, b: Value) -> Option<Value> {
        debug_assert!(
            a.is_int() && b.is_int(),
            "tagged_int_add_checked: both inputs must be Int"
        );
        let (t, overflow) = (a.0 as i64).overflowing_add((b.0 as i64).wrapping_sub(1));
        if overflow {
            None
        } else {
            Some(Value(t as u64))
        }
    }

    /// Difference of two `Int`-tagged Values, or `None` on i63 overflow.
    ///
    /// See [`Value::tagged_int_add_checked`] for the encoding and overflow
    /// details. When present, the result is the exact tagged difference.
    #[inline(always)]
    pub const fn tagged_int_sub_checked(a: Value, b: Value) -> Option<Value> {
        debug_assert!(
            a.is_int() && b.is_int(),
            "tagged_int_sub_checked: both inputs must be Int"
        );
        let (t, overflow) = (a.0 as i64).overflowing_sub((b.0 as i64).wrapping_sub(1));
        if overflow {
            None
        } else {
            Some(Value(t as u64))
        }
    }

    // The OpCode::CmpInt* path does signed comparison directly on the
    // tagged bits (`(l.bits() as i64) < (r.bits() as i64)`); we don't
    // need a separate `tagged_int_cmp` helper.

    // ── Constructors ──────────────────────────────────────────────────────

    /// Build a `Value` carrying an `i63` integer.
    ///
    /// Debug-asserts that `i` is in `[INT_MIN, INT_MAX]`. Values outside
    /// that range are truncated by the encoding shift, so passing one
    /// here is a caller bug. Code that ingests integers from outside the
    /// VM (deserializers, JSON decoders, etc.) should range-check first
    /// or use [`Value::try_int`].
    #[inline(always)]
    pub const fn int(i: i64) -> Self {
        debug_assert!(
            i >= Self::INT_MIN && i <= Self::INT_MAX,
            "Value::int called with i64 outside the i63 range; use Value::try_int at boundaries"
        );
        // Cast is well-defined: `(i as u64) << 1` may technically
        // overflow at the i64 boundary but the result is still a valid
        // u64 bit pattern that decodes back via `as_int`'s arithmetic
        // shift right.
        Value(((i as u64) << 1) | 1)
    }

    /// Build a `Value` carrying an `i63` integer, or `None` if `i` is
    /// outside the i63 range. Use this at boundaries that accept
    /// arbitrary `i64`s (JSON decoders, `Deserialize`, FFI).
    #[inline(always)]
    pub const fn try_int(i: i64) -> Option<Self> {
        if i >= Self::INT_MIN && i <= Self::INT_MAX {
            Some(Value(((i as u64) << 1) | 1))
        } else {
            None
        }
    }

    /// Build a `Value` carrying a boolean.
    #[inline(always)]
    pub const fn bool(b: bool) -> Self {
        if b { Self::TRUE } else { Self::FALSE }
    }

    /// Build a `Value` from a non-null heap pointer.
    ///
    /// Debug-asserts that the pointer is 8-byte aligned (heap allocator
    /// invariant) and non-null (call sites that legitimately have a
    /// nullable pointer should use [`Value::NULL`] explicitly).
    #[inline(always)]
    pub fn object(ptr: HeapPtr) -> Self {
        let bits = ptr.as_ptr() as u64;
        debug_assert!(
            bits != 0,
            "Value::object called with null heap ptr; use Value::NULL"
        );
        debug_assert!(
            bits & 0b111 == 0,
            "Value::object called with mis-aligned heap ptr 0x{bits:x}"
        );
        Value(bits)
    }

    // ── Tag predicates (cheap fast-path discriminators) ───────────────────

    #[inline(always)]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }

    /// `true` for the `OmittedArg` sentinel — the value the VM passes for an
    /// optional argument the caller left out. Native builtins treat an omitted
    /// optional argument the same as an absent one (`None`).
    #[inline(always)]
    pub const fn is_omitted(self) -> bool {
        self.0 == Self::OMITTED_ARG.0
    }

    #[inline(always)]
    pub const fn is_int(self) -> bool {
        self.0 & 1 != 0
    }

    /// True iff `self` is a non-null heap object pointer.
    #[inline(always)]
    pub const fn is_object(self) -> bool {
        self.0 & 0b111 == 0 && self.0 != 0
    }

    // ── Typed accessors (return None on tag mismatch) ─────────────────────

    /// Extract an `i64` if this is an `Int`. Sign-extends from the i63
    /// stored encoding.
    #[inline(always)]
    pub const fn as_int(&self) -> Option<i64> {
        if self.is_int() {
            // Arithmetic shift right preserves sign.
            Some((self.0 as i64) >> 1)
        } else {
            None
        }
    }

    /// Extract a `bool` if this is a `Bool`.
    #[inline(always)]
    pub const fn as_bool(&self) -> Option<bool> {
        match self.0 {
            x if x == Self::FALSE.0 => Some(false),
            x if x == Self::TRUE.0 => Some(true),
            _ => None,
        }
    }

    /// Extract the `HeapPtr` if this is a non-null `Object`. Returns
    /// `None` for `Null` (since the BAML Null is a "null pointer"
    /// encoded as `Value(0)`).
    ///
    /// Takes `&self` so it can be used as a `fn(&Value) -> _` callback
    /// in iterator combinators (`.filter_map(Value::as_object_ptr)`).
    #[inline(always)]
    pub fn as_object_ptr(&self) -> Option<HeapPtr> {
        if self.is_object() {
            // SAFETY: bit pattern was constructed from a valid HeapPtr
            // via [`Value::object`] (which debug-asserts alignment +
            // non-null). The pointer's GC liveness is the caller's
            // concern (same as for the old enum variant). Under
            // `heap_debug` we lose the original epoch (Value is a
            // packed u64 with no room) and pass 0, matching the
            // `resolve_function_constants` reconstruction path.
            let ptr = self.0 as *mut Object;
            #[cfg(feature = "heap_debug")]
            let hp = unsafe { HeapPtr::from_ptr(ptr, 0) };
            #[cfg(not(feature = "heap_debug"))]
            let hp = unsafe { HeapPtr::from_ptr(ptr) };
            Some(hp)
        } else {
            None
        }
    }

    // ── Match on categorical view ─────────────────────────────────────────

    /// Decode into the categorical `ValueKind` for pattern matching.
    /// Use this when you need to branch on the type; use the typed
    /// accessors (`as_int`, `as_bool`, etc.) on the hot path when you
    /// only care about one variant.
    #[inline]
    pub fn kind(&self) -> ValueKind {
        if self.is_int() {
            return ValueKind::Int((self.0 as i64) >> 1);
        }
        match self.0 {
            x if x == Self::NULL.0 => ValueKind::Null,
            x if x == Self::FALSE.0 => ValueKind::Bool(false),
            x if x == Self::TRUE.0 => ValueKind::Bool(true),
            x if x == Self::OMITTED_ARG.0 => ValueKind::OmittedArg,
            _ => {
                // Must be a pointer — low 3 bits zero, non-zero, not a
                // sentinel pattern.
                debug_assert_eq!(self.0 & 0b111, 0, "malformed Value bits 0x{:x}", self.0);
                // SAFETY: see [`Value::as_object_ptr`].
                let ptr = self.0 as *mut Object;
                #[cfg(feature = "heap_debug")]
                let hp = unsafe { HeapPtr::from_ptr(ptr, 0) };
                #[cfg(not(feature = "heap_debug"))]
                let hp = unsafe { HeapPtr::from_ptr(ptr) };
                ValueKind::Object(hp)
            }
        }
    }

    // ── Raw bit access for debugging / advanced use ──────────────────────

    /// The raw `u64` bit pattern. Exposed for diagnostics, formatting,
    /// and concurrency machinery (e.g. `AtomicU64` stores of `Value`).
    /// Prefer the typed accessors for normal use.
    #[inline(always)]
    pub const fn bits(self) -> u64 {
        self.0
    }

    // `from_bits` has no callers yet; the originally-planned atomic-load
    // path will add one when it lands. Re-introduce as `pub(crate) const
    // unsafe fn from_bits(bits: u64) -> Self` at that point so the
    // invariant (bits came from `Value::bits` or a safe constructor) is
    // upheld by callers via `unsafe`.
}

impl Default for Value {
    #[inline(always)]
    #[allow(clippy::inline_always)]
    fn default() -> Self {
        Self::NULL
    }
}

/// Atomic storage for a single [`Value`].
///
/// This is used for heap slots whose value may be read and written by multiple
/// spawned VM fibers (`Cell.value` and `Instance.fields`). It preserves
/// atomicity of the 8-byte tagged value and uses release/acquire ordering so a
/// newly stored object pointer is safely published to a racing reader.
#[repr(transparent)]
pub struct AtomicValueSlot(AtomicU64);

impl AtomicValueSlot {
    #[inline]
    pub const fn new(value: Value) -> Self {
        Self(AtomicU64::new(value.bits()))
    }

    #[inline]
    pub fn load(&self) -> Value {
        Value(self.0.load(Ordering::Acquire))
    }

    #[inline]
    pub fn store(&self, value: Value) {
        self.0.store(value.bits(), Ordering::Release);
    }
}

impl From<Value> for AtomicValueSlot {
    fn from(value: Value) -> Self {
        Self::new(value)
    }
}

impl Clone for AtomicValueSlot {
    fn clone(&self) -> Self {
        Self::new(self.load())
    }
}

impl std::fmt::Debug for AtomicValueSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.load().fmt(f)
    }
}

impl BorshSerialize for AtomicValueSlot {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.load().serialize(writer)
    }
}

impl BorshDeserialize for AtomicValueSlot {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        Value::deserialize_reader(reader).map(Self::new)
    }
}

/// Per-instance "already cleaned" latch backing BEP-042 `cleanup`.
///
/// `cleanup` (the magic finalizer method) must run **at most once** per
/// instance, regardless of whether it is triggered by an explicit
/// `obj.cleanup()`, a `defer { obj.cleanup() }`, or the GC finalizer. This is
/// the shared run-once latch — the analogue of .NET's `GC.SuppressFinalize` bit.
///
/// Atomic because a `defer`-driven cleanup can run on a different `spawn` fiber
/// than another path (and the GC may discover the same instance). The
/// test-and-set in [`Self::begin`] is the synchronization point: only the first
/// caller observes "not yet cleaned" and runs the body.
///
/// Stored inline on [`Instance`](super::Instance) (not in a side table) so it is
/// correct by construction across every GC mode: the collector clones the object
/// when it copies it, and [`Clone`] preserves the bit, so a surviving instance
/// stays cleaned and a reclaimed slot reused for a fresh instance starts
/// uncleaned — no stale-pointer aliasing. Like [`AtomicValueSlot`] it carries
/// manual `Clone`/`Debug`/`Borsh` impls so it can live in the derived `Instance`.
#[repr(transparent)]
pub struct CleanupLatch(AtomicBool);

impl CleanupLatch {
    #[inline]
    pub const fn new(cleaned: bool) -> Self {
        Self(AtomicBool::new(cleaned))
    }

    /// Test-and-set the latch on cleanup entry. Returns `true` iff this is the
    /// *first* call (the caller should run the cleanup body); returns `false` if
    /// the instance was already cleaned (the caller should skip the body).
    ///
    /// The latch is set on entry, not on success: a `cleanup` body that throws
    /// or panics is still considered cleaned and will not be retried (BEP-042).
    #[inline]
    pub fn begin(&self) -> bool {
        // `swap` returns the previous value; the first call observes `false`.
        !self.0.swap(true, Ordering::AcqRel)
    }

    #[inline]
    pub fn is_cleaned(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

impl Clone for CleanupLatch {
    fn clone(&self) -> Self {
        Self::new(self.is_cleaned())
    }
}

impl std::fmt::Debug for CleanupLatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CleanupLatch")
            .field(&self.is_cleaned())
            .finish()
    }
}

impl BorshSerialize for CleanupLatch {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.is_cleaned().serialize(writer)
    }
}

impl BorshDeserialize for CleanupLatch {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        bool::deserialize_reader(reader).map(Self::new)
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind() {
            ValueKind::Null => write!(f, "Null"),
            ValueKind::OmittedArg => write!(f, "OmittedArg"),
            ValueKind::Int(i) => f.debug_tuple("Int").field(&i).finish(),
            ValueKind::Bool(b) => f.debug_tuple("Bool").field(&b).finish(),
            ValueKind::Object(p) => f.debug_tuple("Object").field(&p).finish(),
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind() {
            ValueKind::OmittedArg => write!(f, "<omitted>"),
            ValueKind::Null => write!(f, "null"),
            ValueKind::Int(int) => write!(f, "{int}"),
            ValueKind::Bool(bool) => write!(f, "{bool}"),
            ValueKind::Object(ptr) => write!(f, "{ptr}"),
        }
    }
}

/// Serde proxy for `Value`. Mirrors the categorical shape of the old
/// `enum Value { Null, Int, Bool, Object, OmittedArg }` so on-disk
/// program payloads are wire-compatible with the pre-tagged-ptr
/// encoding. `Object` round-trip will fail because `HeapPtr` itself
/// refuses to serialize — that matches the prior behavior (heap
/// pointers are runtime-only).
#[derive(BorshSerialize, BorshDeserialize)]
enum ValueWire {
    OmittedArg,
    Null,
    Int(i64),
    Bool(bool),
    Object(HeapPtr),
}

impl BorshSerialize for Value {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let proxy = match self.kind() {
            ValueKind::Null => ValueWire::Null,
            ValueKind::OmittedArg => ValueWire::OmittedArg,
            ValueKind::Int(i) => ValueWire::Int(i),
            ValueKind::Bool(b) => ValueWire::Bool(b),
            ValueKind::Object(ptr) => ValueWire::Object(ptr),
        };
        proxy.serialize(writer)
    }
}

impl BorshDeserialize for Value {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let proxy = ValueWire::deserialize_reader(reader)?;
        Ok(match proxy {
            ValueWire::Null => Value::NULL,
            ValueWire::OmittedArg => Value::OMITTED_ARG,
            ValueWire::Int(i) => Value::try_int(i).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "Value::Int payload {i} is outside the i63 range [{}, {}]; \
                         pre-tagged-pointer payloads with |value| >= 2^62 cannot be loaded",
                        Value::INT_MIN,
                        Value::INT_MAX,
                    ),
                )
            })?,
            ValueWire::Bool(b) => Value::bool(b),
            ValueWire::Object(ptr) => Value::object(ptr),
        })
    }
}
