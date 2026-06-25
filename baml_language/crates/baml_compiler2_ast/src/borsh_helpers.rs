//! Borsh `serialize_with` / `deserialize_with` helpers for foreign types the
//! BAML AST embeds but that do not implement `Borsh`: `text_size::TextRange`,
//! `la_arena::Idx`, and `la_arena::Arena`.
//!
//! The precompiled-stdlib artifacts share one serialization format (borsh, as
//! used by the compiled-bytecode prefix in `baml_builtins2_prebuilt`), so the
//! serialized HIR/AST uses borsh too. Foreign types are handled via
//! `#[borsh(serialize_with = "...", deserialize_with = "...")]` field
//! attributes pointing at these helpers, since the orphan rule forbids
//! implementing `Borsh` on them directly.
//!
//! `Arena` is dense and alloc-only (no removal), so iterating it yields elements
//! in `Idx` order `0..len`; re-`alloc`ing them in that order on load reproduces
//! the original indices, keeping every `Idx` valid across a round trip.

use borsh::{
    BorshDeserialize, BorshSerialize,
    io::{Error, Read, Write},
};
use la_arena::{Arena, Idx, RawIdx};
use text_size::TextRange;

fn len_as_u32(len: usize) -> u32 {
    u32::try_from(len).expect("arena length exceeds u32::MAX")
}

// ── TextRange (two u32 offsets) ──────────────────────────────────────────────

pub fn serialize_text_range<W: Write>(range: &TextRange, w: &mut W) -> Result<(), Error> {
    let start: u32 = range.start().into();
    let end: u32 = range.end().into();
    start.serialize(w)?;
    end.serialize(w)
}

pub fn deserialize_text_range<R: Read>(r: &mut R) -> Result<TextRange, Error> {
    let start = u32::deserialize_reader(r)?;
    let end = u32::deserialize_reader(r)?;
    Ok(TextRange::new(start.into(), end.into()))
}

// ── Idx<T> (a dense u32 arena index) ─────────────────────────────────────────

pub fn serialize_idx<T, W: Write>(idx: &Idx<T>, w: &mut W) -> Result<(), Error> {
    idx.into_raw().into_u32().serialize(w)
}

pub fn deserialize_idx<T, R: Read>(r: &mut R) -> Result<Idx<T>, Error> {
    Ok(Idx::from_raw(RawIdx::from_u32(u32::deserialize_reader(r)?)))
}

// ── Option<Idx<T>> ───────────────────────────────────────────────────────────

pub fn serialize_opt_idx<T, W: Write>(idx: &Option<Idx<T>>, w: &mut W) -> Result<(), Error> {
    match idx {
        Some(i) => {
            1u8.serialize(w)?;
            serialize_idx(i, w)
        }
        None => 0u8.serialize(w),
    }
}

pub fn deserialize_opt_idx<T, R: Read>(r: &mut R) -> Result<Option<Idx<T>>, Error> {
    if u8::deserialize_reader(r)? == 0 {
        Ok(None)
    } else {
        Ok(Some(deserialize_idx(r)?))
    }
}

// ── Arena<T> where T: Borsh (dense, alloc-only) ──────────────────────────────

pub fn serialize_arena<T: BorshSerialize, W: Write>(
    arena: &Arena<T>,
    w: &mut W,
) -> Result<(), Error> {
    len_as_u32(arena.len()).serialize(w)?;
    for (_, v) in arena.iter() {
        v.serialize(w)?;
    }
    Ok(())
}

pub fn deserialize_arena<T: BorshDeserialize, R: Read>(r: &mut R) -> Result<Arena<T>, Error> {
    let len = u32::deserialize_reader(r)?;
    let mut arena = Arena::new();
    for _ in 0..len {
        arena.alloc(T::deserialize_reader(r)?);
    }
    Ok(arena)
}

// ── Arena<TextRange> (source-map spans; TextRange isn't Borsh) ───────────────

pub fn serialize_arena_text_range<W: Write>(
    arena: &Arena<TextRange>,
    w: &mut W,
) -> Result<(), Error> {
    len_as_u32(arena.len()).serialize(w)?;
    for (_, range) in arena.iter() {
        serialize_text_range(range, w)?;
    }
    Ok(())
}

pub fn deserialize_arena_text_range<R: Read>(r: &mut R) -> Result<Arena<TextRange>, Error> {
    let len = u32::deserialize_reader(r)?;
    let mut arena = Arena::new();
    for _ in 0..len {
        arena.alloc(deserialize_text_range(r)?);
    }
    Ok(arena)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A tiny struct mirroring how the AST annotates foreign-typed fields, to
    // prove the helper signatures satisfy borsh's derive and round-trip.
    #[derive(BorshSerialize, BorshDeserialize, PartialEq, Debug)]
    struct Sample {
        #[borsh(
            serialize_with = "serialize_text_range",
            deserialize_with = "deserialize_text_range"
        )]
        span: TextRange,
        #[borsh(serialize_with = "serialize_idx", deserialize_with = "deserialize_idx")]
        idx: Idx<u32>,
        #[borsh(
            serialize_with = "serialize_opt_idx",
            deserialize_with = "deserialize_opt_idx"
        )]
        opt_idx: Option<Idx<u32>>,
        #[borsh(
            serialize_with = "serialize_arena",
            deserialize_with = "deserialize_arena"
        )]
        arena: Arena<u32>,
        #[borsh(
            serialize_with = "serialize_arena_text_range",
            deserialize_with = "deserialize_arena_text_range"
        )]
        spans: Arena<TextRange>,
    }

    #[test]
    fn foreign_types_round_trip_through_borsh() {
        let mut arena = Arena::new();
        let i0 = arena.alloc(10u32);
        arena.alloc(20u32);
        let mut spans = Arena::new();
        spans.alloc(TextRange::new(3.into(), 7.into()));

        let sample = Sample {
            span: TextRange::new(5.into(), 11.into()),
            idx: i0,
            opt_idx: Some(i0),
            arena,
            spans,
        };

        let bytes = borsh::to_vec(&sample).unwrap();
        let decoded: Sample = borsh::from_slice(&bytes).unwrap();
        assert_eq!(sample, decoded);
    }
}
