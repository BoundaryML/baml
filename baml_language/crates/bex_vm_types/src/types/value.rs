use std::sync::atomic::{AtomicU64, Ordering};

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
/// | `xxxxx...xx10` (other than 2 and 6)    | `Float` — inline, biased exponent `[768, 1279]`  |
/// | `xxxxx...x100` (other than 4)          | `Float` — inline, biased exponent `[0, 255]`     |
///
/// # On `Float`
///
/// Most floats live inline, "self-tagged" in the spare tag space (Melançon,
/// Serrano and Feeley, *Float Self-Tagging*, OOPSLA 2025): the tag bits of
/// an inline float are bits of its own exponent, so encoding is an add and
/// a rotate, and float arithmetic does not allocate.
///
/// - Tag `10`: biased exponent in `[768, 1279]`, i.e. `|x|` in
///   `[2^-255, 2^257)` — every everyday float. Encoded as
///   `(bits + (256 << 52)).rotate_left(3)`: the offset moves that exponent
///   range to `[1024, 1535]`, whose top two bits are `10`, and the rotation
///   brings those two bits (and the sign, into bit 2) to the bottom.
/// - Tag `100`: biased exponent in `[0, 255]`: `±0.0`, subnormals and
///   `|x| < 2^-767`. Encoded as `((bits ^ 1) + (1024 << 52)).rotate_left(4)`.
///   The `^ 1` keeps `+0.0` off the `TRUE` word `4`; the one float that lands
///   there instead is the smallest positive subnormal.
///
/// Every other float is heap-boxed as `Object::Float(f64)` behind the
/// pointer arm: NaN, `±inf`, `|x| >= 2^257`, `|x|` in `[2^-767, 2^-255)`,
/// and the three floats whose encoding would be exactly a singleton word
/// (`±2^-255` and the smallest positive subnormal).
///
/// The representation is canonical: every float bit pattern has exactly one
/// encoding, inline when the rules above allow it and boxed only otherwise.
/// Build floats only through the heap's `float_value` (inline, else boxed)
/// or [`Value::try_inline_float`]; read them only through
/// [`Value::as_float`]. [`ValueKind::Float`] is the inline form; a boxed
/// float reaches a `kind()` match as [`ValueKind::Object`].
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
/// floats) and bit equality for inline floats (so `-0.0` and `0.0` differ
/// here). Content equality for strings, arrays, floats, etc. is handled at
/// the user-visible `==` operator in the VM dispatch, not here.
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
    /// An inline float. Boxed floats (NaN, `±inf`, extreme magnitudes) are
    /// [`ValueKind::Object`]s holding an `Object::Float`; [`Value::as_float`]
    /// reads either.
    Float(f64),
    Object(HeapPtr),
}

/// Exponent offset of the tag-`10` inline float form (see [`Value`]).
const FLOAT_HI_OFFSET: u64 = 256 << 52;
/// Exponent offset of the tag-`100` inline float form (see [`Value`]).
const FLOAT_LO_OFFSET: u64 = 1024 << 52;
/// Mantissa bit flipped in the tag-`100` form so `+0.0` does not encode to
/// the `TRUE` word.
const FLOAT_LO_FLIP: u64 = 1;

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

    /// The inline encoding of `f`, or `None` when `f` must be heap-boxed
    /// (NaN, `±inf`, magnitudes outside the two inline exponent ranges, and
    /// the three floats whose encoding would collide with a singleton). See
    /// the type docs for the encoding.
    ///
    /// To *build* a float value, use the heap's `float_value`, which boxes
    /// exactly the floats this rejects, so no representable float is ever
    /// boxed.
    #[inline(always)]
    pub const fn try_inline_float(f: f64) -> Option<Self> {
        let bits = f.to_bits();
        let hi = bits.wrapping_add(FLOAT_HI_OFFSET).rotate_left(3);
        // Low bits `10`, and not the `FALSE` / `OMITTED_ARG` words 2 and 6.
        if hi & 0b11 == 0b10 && hi > 7 {
            return Some(Value(hi));
        }
        let lo = (bits ^ FLOAT_LO_FLIP)
            .wrapping_add(FLOAT_LO_OFFSET)
            .rotate_left(4);
        // Low bits `100`, and not the `TRUE` word 4.
        if lo & 0b111 == 0b100 && lo > 7 {
            return Some(Value(lo));
        }
        None
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

    /// True iff `self` is an inline float. A boxed float is an object; use
    /// [`Value::as_float`] to accept both.
    #[inline(always)]
    pub const fn is_inline_float(self) -> bool {
        // Low bit clear (not an int), low 3 bits not `000` (not an object or
        // null), and not one of the singleton words 2 / 4 / 6.
        self.0 & 1 == 0 && self.0 & 0b110 != 0 && self.0 > 7
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

    /// Extract an `f64` if this is an inline float. Bit-exact: the result has
    /// the same 64 bits as the float that was encoded.
    #[inline(always)]
    pub const fn as_inline_float(&self) -> Option<f64> {
        let v = self.0;
        // Tag `10` first: every everyday magnitude.
        if v & 0b11 == 0b10 && v > 7 {
            return Some(f64::from_bits(
                v.rotate_right(3).wrapping_sub(FLOAT_HI_OFFSET),
            ));
        }
        // Tag `100`: zero, subnormals, tiny magnitudes.
        if v & 0b111 == 0b100 && v > 7 {
            return Some(f64::from_bits(
                v.rotate_right(4).wrapping_sub(FLOAT_LO_OFFSET) ^ FLOAT_LO_FLIP,
            ));
        }
        None
    }

    /// Extract an `f64` if this is a float, inline or heap-boxed: the one
    /// accessor for reading a float out of a `Value`.
    ///
    /// Returns `None` for every other kind, including ints — callers that
    /// want int→float promotion must combine this with `as_int`.
    #[inline(always)]
    pub fn as_float(&self) -> Option<f64> {
        if let Some(f) = self.as_inline_float() {
            return Some(f);
        }
        self.as_boxed_float()
    }

    /// The boxed half of [`Value::as_float`]: `Some` iff `self` points at an
    /// `Object::Float`.
    #[inline]
    fn as_boxed_float(self) -> Option<f64> {
        let ptr = self.as_object_ptr()?;
        // SAFETY: a heap pointer held in a live `Value` is valid for reads;
        // its GC liveness is the caller's concern, as for every pointer read.
        match unsafe { ptr.get() } {
            Object::Float(f) => Some(*f),
            _ => None,
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
            x if x & 0b111 != 0 => match self.as_inline_float() {
                Some(f) => ValueKind::Float(f),
                None => unreachable!("malformed Value bits 0x{x:x}"),
            },
            _ => {
                // Must be a pointer — not a sentinel pattern.
                debug_assert!(self.is_object(), "malformed Value bits 0x{:x}", self.0);
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

    /// The value whose bits are `bits`, as [`Self::bits`] gave them.
    ///
    /// # Safety
    ///
    /// `bits` came from [`Self::bits`] of a value that is still live: a heap
    /// pointer decoded from any other bits names nothing.
    #[inline(always)]
    #[allow(unsafe_code)]
    pub const unsafe fn from_bits(bits: u64) -> Self {
        Self(bits)
    }
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
/// spawned VM fibers (`Cell.value` and package slots). It preserves
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

    /// [`Self::load`] for a slot no other thread can reach, which needs no
    /// ordering. The slot stays atomic, but a wrong caller can read a pointer
    /// before the object it points to is published: a race on that object.
    #[inline]
    pub fn load_relaxed(&self) -> Value {
        Value(self.0.load(Ordering::Relaxed))
    }

    /// [`Self::store`] for a slot no other thread can reach.
    #[inline]
    pub fn store_relaxed(&self, value: Value) {
        self.0.store(value.bits(), Ordering::Relaxed);
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

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind() {
            ValueKind::Null => write!(f, "Null"),
            ValueKind::OmittedArg => write!(f, "OmittedArg"),
            ValueKind::Int(i) => f.debug_tuple("Int").field(&i).finish(),
            ValueKind::Bool(b) => f.debug_tuple("Bool").field(&b).finish(),
            ValueKind::Float(x) => f.debug_tuple("Float").field(&x).finish(),
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
            ValueKind::Float(x) => write!(f, "{x}"),
            ValueKind::Object(ptr) => write!(f, "{ptr}"),
        }
    }
}

/// Serde proxy for `Value`. Mirrors the categorical shape of the old
/// `enum Value { Null, Int, Bool, Object, OmittedArg }` so on-disk
/// program payloads are wire-compatible with the pre-tagged-ptr
/// encoding. `Object` round-trip will fail because `HeapPtr` itself
/// refuses to serialize — that matches the prior behavior (heap
/// pointers are runtime-only). `Float` (an inline float) comes last so the
/// earlier variant indices keep their meaning.
#[derive(BorshSerialize, BorshDeserialize)]
enum ValueWire {
    OmittedArg,
    Null,
    Int(i64),
    Bool(bool),
    Object(HeapPtr),
    Float(f64),
}

impl BorshSerialize for Value {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let proxy = match self.kind() {
            ValueKind::Null => ValueWire::Null,
            ValueKind::OmittedArg => ValueWire::OmittedArg,
            ValueKind::Int(i) => ValueWire::Int(i),
            ValueKind::Bool(b) => ValueWire::Bool(b),
            ValueKind::Float(f) => ValueWire::Float(f),
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
            ValueWire::Float(f) => Value::try_inline_float(f).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Value::Float payload {f} has no inline encoding"),
                )
            })?,
        })
    }
}

#[cfg(test)]
mod float_encoding_tests {
    use super::{Value, ValueKind};

    /// `splitmix64`: a deterministic stream of well-mixed `u64`s, so the
    /// property tests below cover the whole bit space without a dependency.
    struct SplitMix(u64);

    impl SplitMix {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }
    }

    const EXP_MASK: u64 = 0x7FF << 52;

    fn biased_exponent(bits: u64) -> u64 {
        (bits & EXP_MASK) >> 52
    }

    /// The documented rule for which floats are inline.
    fn expected_inline(bits: u64) -> bool {
        let e = biased_exponent(bits);
        let pow_m255 = 768_u64 << 52; // 2^-255
        match e {
            768..=1279 => bits & !(1 << 63) != pow_m255,
            0..=255 => bits != 1,
            _ => false,
        }
    }

    /// Every property one float must satisfy: the inline/boxed decision
    /// follows the documented rule, an inline encoding round-trips to the
    /// identical 64 bits, and it never looks like an int, an object, or a
    /// singleton.
    fn check(bits: u64) {
        let f = f64::from_bits(bits);
        let encoded = Value::try_inline_float(f);
        assert_eq!(
            encoded.is_some(),
            expected_inline(bits),
            "inline decision for bits 0x{bits:016x} ({f:e})"
        );
        let Some(v) = encoded else { return };
        assert!(v.is_inline_float(), "0x{bits:016x}");
        assert!(
            !v.is_int() && !v.is_object() && !v.is_null(),
            "0x{bits:016x}"
        );
        assert!(
            ![Value::NULL, Value::FALSE, Value::TRUE, Value::OMITTED_ARG].contains(&v),
            "0x{bits:016x} encodes to a singleton"
        );
        assert_eq!(v.as_bool(), None);
        assert_eq!(v.as_int(), None);
        assert_eq!(v.as_object_ptr(), None);
        assert_eq!(v.as_inline_float().map(f64::to_bits), Some(bits));
        assert_eq!(v.as_float().map(f64::to_bits), Some(bits));
        match v.kind() {
            ValueKind::Float(g) => assert_eq!(g.to_bits(), bits),
            other => panic!("0x{bits:016x} decodes to {other:?}"),
        }
    }

    #[test]
    fn edge_values_round_trip_bit_identically() {
        let pow = |e: i32| 2.0_f64.powi(e);
        let edges = [
            0.0,
            -0.0,
            1.0,
            -1.0,
            0.5,
            1.5,
            std::f64::consts::PI,
            -std::f64::consts::E,
            f64::MIN_POSITIVE,
            -f64::MIN_POSITIVE,
            f64::from_bits(1),                     // smallest subnormal (boxed)
            -f64::from_bits(1),                    // inline
            f64::from_bits(0x000F_FFFF_FFFF_FFFF), // largest subnormal
            -f64::from_bits(0x000F_FFFF_FFFF_FFFF),
            pow(-255),  // would encode to FALSE: boxed
            -pow(-255), // would encode to OMITTED_ARG: boxed
            pow(-255) * (1.0 + f64::EPSILON),
            pow(257), // first magnitude above the inline range
            -pow(257),
            pow(257) * (1.0 - f64::EPSILON / 2.0), // largest inline
            pow(-256),
            pow(-767),
            pow(-768),
            pow(-768) * (1.0 + f64::EPSILON),
            f64::MAX,
            f64::MIN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
            -f64::NAN,
            f64::from_bits(0x7FF0_0000_0000_0001), // signalling NaN
            f64::from_bits(0x7FF8_DEAD_BEEF_0001), // NaN payload
            f64::from_bits(0xFFFF_FFFF_FFFF_FFFF),
        ];
        for f in edges {
            check(f.to_bits());
        }
        // Spot checks of the documented boxed set.
        for boxed in [
            pow(-255),
            -pow(-255),
            f64::from_bits(1),
            f64::NAN,
            f64::INFINITY,
        ] {
            assert!(Value::try_inline_float(boxed).is_none(), "{boxed:e}");
        }
        for inline in [0.0, -0.0, 1.0, -1e77, 1e-76, 1e-300, 3.5e-320] {
            assert!(Value::try_inline_float(inline).is_some(), "{inline:e}");
        }
    }

    #[test]
    fn exponent_range_edges_round_trip() {
        for e in [
            0_u64, 1, 254, 255, 256, 257, 767, 768, 769, 1023, 1278, 1279, 1280, 2046, 2047,
        ] {
            for mantissa in [
                0,
                1,
                2,
                0x8_0000_0000_0000,
                0xF_FFFF_FFFF_FFFE,
                0xF_FFFF_FFFF_FFFF,
            ] {
                for sign in [0, 1_u64 << 63] {
                    check(sign | (e << 52) | mantissa);
                }
            }
        }
    }

    #[test]
    fn random_bit_patterns_round_trip_bit_identically() {
        let mut rng = SplitMix(0x5EED_F10A_7000_0001);
        for _ in 0..2_000_000 {
            let bits = rng.next();
            check(bits);
            // Also aim at each inline range: uniform bits only rarely land
            // in the tag-`100` range.
            let e = match rng.next() % 3 {
                0 => rng.next() % 256,
                1 => 768 + rng.next() % 512,
                _ => rng.next() % 2048,
            };
            check((bits & !EXP_MASK) | (e << 52));
        }
    }

    /// Canonical: every word in the inline-float tag space is the encoding
    /// of exactly the float it decodes to, so no float has two inline forms
    /// and decoding is a bijection onto the inline floats.
    #[test]
    fn every_float_tagged_word_is_canonical() {
        let mut rng = SplitMix(0xCA70_0000_0000_0002);
        for _ in 0..2_000_000 {
            let raw = rng.next();
            for word in [(raw & !0b11) | 0b10, (raw & !0b111) | 0b100] {
                if word <= 7 {
                    continue;
                }
                let v = Value(word);
                assert!(v.is_inline_float(), "0x{word:016x}");
                let f = v.as_inline_float().expect("float-tagged word decodes");
                assert_eq!(
                    Value::try_inline_float(f),
                    Some(v),
                    "0x{word:016x} decodes to {f:e}, which encodes differently"
                );
            }
        }
    }

    /// Ints, pointers and singletons keep their encoding and never read as a
    /// float.
    #[test]
    fn non_float_words_are_not_floats() {
        for v in [Value::NULL, Value::FALSE, Value::TRUE, Value::OMITTED_ARG] {
            assert!(!v.is_inline_float());
            assert_eq!(v.as_inline_float(), None);
        }
        for i in [0, 1, -1, Value::INT_MAX, Value::INT_MIN, 42] {
            let v = Value::int(i);
            assert!(!v.is_inline_float());
            assert_eq!(v.as_int(), Some(i));
            assert_eq!(v.as_float(), None);
        }
        let mut rng = SplitMix(0x0B1E_C700_0000_0003);
        for _ in 0..100_000 {
            let aligned = rng.next() & !0b111;
            if aligned != 0 {
                assert!(!Value(aligned).is_inline_float());
                assert!(Value(aligned).is_object());
            }
            assert!(!Value(rng.next() | 1).is_inline_float());
        }
    }

    #[test]
    fn inline_float_borsh_round_trip() {
        for f in [0.0, -0.0, 1.25, -3.0e-310, 6.02e23] {
            let v = Value::try_inline_float(f).unwrap();
            let bytes = borsh::to_vec(&v).unwrap();
            let back: Value = borsh::from_slice(&bytes).unwrap();
            assert_eq!(back, v);
        }
    }
}
