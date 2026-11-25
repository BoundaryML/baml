//! Maps: insertion-ordered hash tables.
//!
//! [`crate::layout`] gives the layout. A map's records are dense, in the
//! order their keys were first set, with a hash kept beside each. Up to
//! [`SMALL_ENTRIES`] records are searched by comparing every kept hash at
//! once; past that an index object maps a hash to a record's number by
//! linear probing. Nothing here depends on an object's address, so a
//! collection moves a map's objects without rehashing anything.
//!
//! Deleting zeroes the record and its hash and marks the index slot, so the
//! other records keep their numbers. When the records run out, the dead
//! ones are squeezed out in place if they are at least half, and otherwise
//! the entries double. Either way the index is rebuilt from the kept
//! hashes alone: no key is read.
//!
//! A record is two values, the key and what it is set to. A `string` key
//! is a reference to a string that holds its own text, and an `int`,
//! `bool`, enum variant or `float` is its bits, a float's made canonical
//! first. Keys of any other type are not handled here.
//!
//! Compiled code searches a map through the leaf probes at the end of this
//! file. What a probe returns is the address of a record's value, a slot,
//! good only until the next allocation.

#![allow(
    clippy::cast_possible_truncation,
    reason = "counts and hashes are 32 bits by the layout"
)]

use std::sync::atomic::{AtomicU64, Ordering};

use crate::{
    heap::{Mutator, Value, load, store},
    layout::{
        ENTRIES_CAPACITY, ENTRIES_RECORDS, ENTRY_KEY, ENTRY_SIZE, ENTRY_VALUE, HASH_SET,
        INDEX_SLOT_COUNT, INDEX_SLOTS, MAP_ENTRIES, MAP_EPOCH, MAP_INDEX, MAP_KEY_TY, MAP_LEN,
        MAP_MASK, MAP_SIZE, MAP_USED, MAP_VALUE_TY, SMALL_ENTRIES, STRING_HASH_MASK,
        STRING_LENGTHS, STRING_TAGS_END, TAG_ENUM_VARIANT, TAG_FLOAT, TAG_MAP, TAG_MAP_ENTRIES,
        TAG_MAP_INDEX, TAG_STRING, TYPE_ID_MASK, VARIANT_INDEX, WORD, entries_size,
    },
    string,
};

const S1: u64 = 0xa076_1d64_78bd_642f;
const S2: u64 = 0xe703_7ed1_a0b4_28db;

/// What every hash of this process is mixed with. Each mutator holds a
/// copy where the probes read it.
static SEED: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);

/// The seed of this process's hashes.
pub fn seed() -> u64 {
    SEED.load(Ordering::Relaxed)
}

/// Pick this process's seed. Called once, before any mutator is made.
pub fn randomize_seed() {
    use std::hash::{BuildHasher, Hasher};
    let random = std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish();
    SEED.store(random | 1, Ordering::Relaxed);
}

/// The entries every empty map shares: no capacity, so the first key set
/// makes the map its own, and eight zero hashes for a search to compare.
#[repr(C, align(16))]
struct EmptyEntries([usize; entries_size(SMALL_ENTRIES) / WORD]);

static EMPTY_ENTRIES: EmptyEntries = {
    let mut words = [0; entries_size(SMALL_ENTRIES) / WORD];
    words[0] = TAG_MAP_ENTRIES;
    EmptyEntries(words)
};

/// The address of the entries of an empty map.
pub fn empty_entries() -> usize {
    std::ptr::from_ref(&EMPTY_ENTRIES) as usize
}

/// How a map's keys are hashed and compared.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyKind {
    /// A reference to a string, compared by text.
    Str,
    /// The bits of a `float`: every NaN is one key, and so are both zeros.
    Float,
    /// The bits of an `int` or a `bool`.
    Word,
    /// The address of an enum's variant: two keys are one when they are
    /// the same address, and a key is hashed by the variant's position,
    /// which stays what it is when the variant's object moves.
    Variant,
}

impl KeyKind {
    /// The kind of the keys `key` is one of, by its tag.
    fn of_value(key: Value) -> Self {
        match key.id() {
            TAG_FLOAT => Self::Float,
            TAG_ENUM_VARIANT => Self::Variant,
            string if string < STRING_TAGS_END && key.is_pointer() => Self::Str,
            _ => Self::Word,
        }
    }

    /// The payload a key is kept and searched for as.
    pub fn canonical(self, key: usize) -> usize {
        match self {
            Self::Float => {
                let value = f64::from_bits(key as u64);
                if value.is_nan() {
                    f64::NAN.to_bits() as usize
                } else if value == 0.0 {
                    0
                } else {
                    key
                }
            }
            _ => key,
        }
    }

    /// The hash of the key whose payload is `key`, as entries keep it.
    ///
    /// # Safety
    ///
    /// A `Str` key is a live string object, and a `Variant` key a live
    /// variant object.
    pub unsafe fn hash(self, key: usize) -> u32 {
        match self {
            // SAFETY: the caller's contract.
            Self::Str => unsafe { str_hash(key) | HASH_SET },
            // SAFETY: the caller's contract.
            Self::Variant => int_hash(unsafe { load(key + VARIANT_INDEX) } as u64),
            _ => int_hash(key as u64),
        }
    }
}

fn mum(left: u64, right: u64) -> u64 {
    let product = u128::from(left) * u128::from(right);
    (product as u64) ^ ((product >> 64) as u64)
}

/// The hash of an `int` key, as entries keep it.
pub fn int_hash(key: u64) -> u32 {
    (mum(key ^ seed(), S1) >> 32) as u32 | HASH_SET
}

/// The hash a string of this text caches in its header: never zero. The
/// text is read as whole words, the last one filled out with zeros.
pub fn text_hash(text: &[u8]) -> u32 {
    let word = |index: usize| {
        let mut bytes = [0; WORD];
        let rest = &text[(index * WORD).min(text.len())..];
        let count = rest.len().min(WORD);
        bytes[..count].copy_from_slice(&rest[..count]);
        u64::from_le_bytes(bytes)
    };
    let (mut seed, mut first) = (seed(), 0);
    let count = text.len().div_ceil(WORD);
    while first + 2 < count {
        seed = mum(word(first) ^ S1, word(first + 1) ^ seed);
        first += 2;
    }
    let hash = mum(
        S2 ^ text.len() as u64,
        mum(word(first) ^ S1, word(first + 1) ^ seed),
    );
    match (hash >> 32) as u32 & STRING_HASH_MASK {
        0 => 1,
        hash => hash,
    }
}

/// The hash of a string, from its header when it has been computed before
/// and kept there otherwise.
///
/// # Safety
///
/// `string` is a live string object.
pub unsafe fn str_hash(string: usize) -> u32 {
    // SAFETY: the caller's contract. The hash goes in bits of the header
    // nothing else uses.
    unsafe {
        let header = load(string);
        let cached = (header >> 32) as u32 & STRING_HASH_MASK;
        if cached != 0 {
            return cached;
        }
        let hash = text_hash(string::text(string).as_bytes());
        store(string, header | (hash as usize) << 32);
        hash
    }
}

/// What the probes call for a string that is a slice.
unsafe extern "C" fn str_hash_slow(string: usize) -> u32 {
    // SAFETY: the probe's caller passed a live string.
    unsafe { str_hash(string) }
}

unsafe fn read32(address: usize) -> u32 {
    // SAFETY: the caller's contract.
    unsafe { *(address as *const u32) }
}

unsafe fn write32(address: usize, value: u32) {
    // SAFETY: the caller's contract.
    unsafe { *(address as *mut u32) = value }
}

/// The address of record `number` of the entries at `entries`.
fn record(entries: usize, number: usize) -> usize {
    entries + ENTRIES_RECORDS + number * ENTRY_SIZE
}

/// The address of the kept hash of record `number` of entries with room
/// for `capacity`.
fn hash_slot(entries: usize, capacity: usize, number: usize) -> usize {
    record(entries, capacity) + number * 4
}

/// Write a record. Every store into entries is here or in
/// [`store_value`].
///
/// # Safety
///
/// `at` is a record of live entries.
unsafe fn store_record(at: usize, key: Value, value: Value) {
    // SAFETY: the caller's contract.
    unsafe {
        Value::store(at + ENTRY_KEY, key);
        Value::store(at + ENTRY_VALUE, value);
    }
}

/// Replace the value at `slot`, the address of a record's value.
///
/// # Safety
///
/// `slot` is the value of a record of live entries.
pub unsafe fn store_value(slot: usize, value: Value) {
    // SAFETY: the caller's contract.
    unsafe { Value::store(slot, value) }
}

/// The capacity entries have to hold `count` records.
fn capacity_for(count: usize) -> usize {
    count.next_power_of_two().max(SMALL_ENTRIES)
}

/// The bytes of the index of entries with room for `capacity`: none while
/// they are searched without one, and otherwise two slots a record.
fn index_size(capacity: usize) -> usize {
    if capacity > SMALL_ENTRIES {
        crate::layout::index_size(2 * capacity)
    } else {
        0
    }
}

/// Make `size` bytes at `entries` into empty entries with room for
/// `capacity`, and the index after them if they have one. Returns the
/// index, or zero.
///
/// # Safety
///
/// `entries` is `entries_size(capacity) + index_size(capacity)` fresh bytes.
unsafe fn format(entries: usize, capacity: usize) -> usize {
    // SAFETY: the caller's contract.
    unsafe {
        let size = entries_size(capacity);
        std::ptr::write_bytes(entries as *mut u8, 0, size + index_size(capacity));
        store(entries, TAG_MAP_ENTRIES);
        store(entries + ENTRIES_CAPACITY, capacity);
        if capacity <= SMALL_ENTRIES {
            return 0;
        }
        let index = entries + size;
        store(index, TAG_MAP_INDEX);
        store(index + INDEX_SLOT_COUNT, 2 * capacity);
        index
    }
}

/// Put record `number`, whose hash is `hash`, in the index.
///
/// # Safety
///
/// `index` is a live index of `mask + 1` slots, not all in use.
unsafe fn index_insert(index: usize, mask: usize, hash: u32, number: usize) {
    let slots = index + INDEX_SLOTS;
    let mut at = hash as usize & mask;
    // SAFETY: the caller's contract.
    unsafe {
        while load(slots + at * WORD) != 0 {
            at = (at + 1) & mask;
        }
        store(slots + at * WORD, (number + 1) << 32 | hash as usize);
    }
}

/// A new map with room for `count` keys, in one allocation. May collect.
pub fn new(mutator: &mut Mutator, count: usize) -> usize {
    let map = mutator.allocate(size_for(count));
    // SAFETY: the allocation is the room the map takes.
    unsafe { new_at(map, count) }
}

/// The bytes of a map with room for `count` keys, as [`new_at`] makes it.
pub fn size_for(count: usize) -> usize {
    if count == 0 {
        MAP_SIZE
    } else {
        let capacity = capacity_for(count);
        MAP_SIZE + entries_size(capacity) + index_size(capacity)
    }
}

/// Make the `size_for(count)` bytes at `map` an empty map with room for
/// `count` keys. Allocates nothing: for a native that builds a tree in
/// room it reserved.
///
/// # Safety
///
/// `map` is `size_for(count)` fresh bytes.
pub unsafe fn new_at(map: usize, count: usize) -> usize {
    // SAFETY: the caller's contract.
    unsafe {
        std::ptr::write_bytes(map as *mut u8, 0, MAP_SIZE);
        store(map, TAG_MAP);
        store(map + MAP_KEY_TY, 0);
        store(map + MAP_VALUE_TY, 0);
        if count == 0 {
            store(map + MAP_ENTRIES, empty_entries());
            store(map + MAP_INDEX, 0);
        } else {
            let entries = map + MAP_SIZE;
            let index = format(entries, capacity_for(count));
            store(map + MAP_ENTRIES, entries);
            store(map + MAP_INDEX, index);
        }
        let capacity = load(load(map + MAP_ENTRIES) + ENTRIES_CAPACITY);
        write32(map + MAP_MASK, (2 * capacity).wrapping_sub(1) as u32);
    }
    map
}

/// Set `key` to `value` in the map at `map`, which has room for the key.
/// Allocates nothing. A key already there keeps its place and takes the
/// value.
///
/// # Safety
///
/// `map` is a live map of string keys with an unused record, `key` a
/// string that holds its own text, and `value` a value of the map's value
/// type.
pub unsafe fn set_in_place(map: usize, key: usize, value: Value) {
    // SAFETY: the caller's contract.
    unsafe {
        debug_assert_eq!(load(key) & TYPE_ID_MASK, TAG_STRING);
        let hash = KeyKind::Str.hash(key);
        if let Some(found) = find(map, KeyKind::Str, key, hash) {
            store_value(
                record(load(map + MAP_ENTRIES), found.number) + ENTRY_VALUE,
                value,
            );
            return;
        }
        let (number, entries) = used(map);
        let capacity = load(entries + ENTRIES_CAPACITY);
        assert!(number < capacity, "a map built in place has room");
        store_record(record(entries, number), Value::reference(key), value);
        write32(hash_slot(entries, capacity, number), hash);
        let index = load(map + MAP_INDEX);
        if index != 0 {
            index_insert(index, read32(map + MAP_MASK) as usize, hash, number);
        }
        write32(map + MAP_USED, number as u32 + 1);
        write32(map + MAP_LEN, read32(map + MAP_LEN) + 1);
    }
}

/// How many keys the map at `map` has.
///
/// # Safety
///
/// `map` is a live map.
pub unsafe fn len(map: usize) -> usize {
    // SAFETY: the caller's contract.
    unsafe { read32(map + MAP_LEN) as usize }
}

/// How many records of the map at `map` have been used, dead ones
/// included, and its entries.
unsafe fn used(map: usize) -> (usize, usize) {
    // SAFETY: the caller's contract.
    unsafe { (read32(map + MAP_USED) as usize, load(map + MAP_ENTRIES)) }
}

/// Whether the payload of the key of the record at `at` is `key`, or is a
/// string of the same text.
unsafe fn same_key(kind: KeyKind, at: usize, key: usize) -> bool {
    // SAFETY: a record whose hash matched holds a key.
    unsafe {
        let kept = Value::payload_at(at + ENTRY_KEY);
        kept == key
            || (kind == KeyKind::Str
                && load(kept + STRING_LENGTHS) == load(key + STRING_LENGTHS)
                && string::text(kept) == string::text(key))
    }
}

/// Where a key is: the number of its record and, when the map has an
/// index, the address of the slot naming it.
struct Found {
    number: usize,
    slot: usize,
}

/// Search the map at `map` for the key whose payload is `key`, canonical
/// and of hash `hash`.
///
/// # Safety
///
/// `map` is a live map of keys of `kind`, and `key` the payload of one.
unsafe fn find(map: usize, kind: KeyKind, key: usize, hash: u32) -> Option<Found> {
    // SAFETY: the caller's contract and the layout.
    unsafe {
        let (entries, index) = (load(map + MAP_ENTRIES), load(map + MAP_INDEX));
        if index == 0 {
            // No more than the smallest capacity: an empty map's shared
            // entries hold that many zero hashes.
            return (0..SMALL_ENTRIES)
                .find(|&number| {
                    read32(hash_slot(entries, SMALL_ENTRIES, number)) == hash
                        && same_key(kind, record(entries, number), key)
                })
                .map(|number| Found { number, slot: 0 });
        }
        let mask = read32(map + MAP_MASK) as usize;
        let slots = index + INDEX_SLOTS;
        let mut at = hash as usize & mask;
        loop {
            let slot = load(slots + at * WORD);
            if slot == 0 {
                return None;
            }
            if slot as u32 == hash {
                let number = (slot >> 32) - 1;
                if same_key(kind, record(entries, number), key) {
                    return Some(Found {
                        number,
                        slot: slots + at * WORD,
                    });
                }
            }
            at = (at + 1) & mask;
        }
    }
}

/// The address of the value of the key whose payload is `key` in the map
/// at `map`, a slot, good until the next allocation.
///
/// # Safety
///
/// `map` is a live map of keys of `kind`, and `key` the payload of one.
unsafe fn slot_of(map: usize, kind: KeyKind, key: usize) -> Option<usize> {
    let key = kind.canonical(key);
    // SAFETY: the caller's contract.
    unsafe {
        find(map, kind, key, kind.hash(key))
            .map(|found| record(load(map + MAP_ENTRIES), found.number) + ENTRY_VALUE)
    }
}

/// The address of the value of `key` in the map at `map`, a slot, good
/// until the next allocation.
///
/// # Safety
///
/// `map` is a live map, and `key` a value of its key type.
pub unsafe fn get(map: usize, key: Value) -> Option<usize> {
    // SAFETY: the caller's contract.
    unsafe { slot_of(map, KeyKind::of_value(key), key.payload) }
}

/// Squeeze the dead records out of the entries of the map at `map`.
unsafe fn compact(map: usize) {
    // SAFETY: the caller's contract and the layout: a live record moves to
    // a lower number or stays.
    unsafe {
        let (used, entries) = used(map);
        let capacity = load(entries + ENTRIES_CAPACITY);
        let mut live = 0;
        for number in 0..used {
            let hash = read32(hash_slot(entries, capacity, number));
            if hash == 0 {
                continue;
            }
            if live != number {
                let from = record(entries, number);
                store_record(
                    record(entries, live),
                    Value::load(from + ENTRY_KEY),
                    Value::load(from + ENTRY_VALUE),
                );
                write32(hash_slot(entries, capacity, live), hash);
            }
            live += 1;
        }
        for number in live..used {
            store_record(record(entries, number), Value::NULL, Value::NULL);
            write32(hash_slot(entries, capacity, number), 0);
        }
        write32(map + MAP_USED, live as u32);
        write32(map + MAP_EPOCH, read32(map + MAP_EPOCH).wrapping_add(1));
        let index = load(map + MAP_INDEX);
        if index != 0 {
            let mask = read32(map + MAP_MASK) as usize;
            std::ptr::write_bytes((index + INDEX_SLOTS) as *mut usize, 0, mask + 1);
            for number in 0..live {
                index_insert(
                    index,
                    mask,
                    read32(hash_slot(entries, capacity, number)),
                    number,
                );
            }
        }
    }
}

/// Make room for one more record in the map at `*map`, whose records are
/// all used. May collect: `map` is a root of the caller's.
unsafe fn grow(mutator: &mut Mutator, map: *const usize) {
    // SAFETY: the caller's contract and the layout. The collector writes
    // the root, so it is read again after the allocation.
    unsafe {
        let before = map.read_volatile();
        let (count, live) = (used(before).0, len(before));
        if count != 0 && count - live >= count.div_ceil(2) {
            compact(before);
            return;
        }
        let capacity = capacity_for(live + 1);
        let entries = mutator.allocate(entries_size(capacity) + index_size(capacity));
        let index = format(entries, capacity);
        let mask = 2 * capacity - 1;
        let map = map.read_volatile();
        let old = load(map + MAP_ENTRIES);
        let old_capacity = load(old + ENTRIES_CAPACITY);
        let mut next = 0;
        for number in 0..count {
            let hash = read32(hash_slot(old, old_capacity, number));
            if hash == 0 {
                continue;
            }
            let from = record(old, number);
            store_record(
                record(entries, next),
                Value::load(from + ENTRY_KEY),
                Value::load(from + ENTRY_VALUE),
            );
            write32(hash_slot(entries, capacity, next), hash);
            if index != 0 {
                index_insert(index, mask, hash, next);
            }
            next += 1;
        }
        if next != count {
            write32(map + MAP_EPOCH, read32(map + MAP_EPOCH).wrapping_add(1));
        }
        store(map + MAP_ENTRIES, entries);
        store(map + MAP_INDEX, index);
        write32(map + MAP_USED, next as u32);
        write32(map + MAP_MASK, mask as u32);
    }
}

/// Add `key`, which the map at `map` does not have, with `value`. May
/// collect.
///
/// # Safety
///
/// `map` is a live map that does not have `key`, a value of its key type
/// whose payload is canonical and of hash `hash`; `value` is a value of
/// its value type.
unsafe fn insert_new(mutator: &mut Mutator, map: usize, key: Value, hash: u32, value: Value) {
    let mut held = [map, key.payload, value.payload];
    let key_is_string = KeyKind::of_value(key) == KeyKind::Str;
    // A key that is an object is a string or an enum's variant.
    let roots = 1 + usize::from(key.is_pointer()) + usize::from(value.is_pointer());
    mutator.root(&mut held[0]);
    if key.is_pointer() {
        mutator.root(&mut held[1]);
    }
    if value.is_pointer() {
        mutator.root(&mut held[2]);
    }
    let mut key_tag = key.tag;
    // SAFETY: the caller's contract; what is allocated here is reached
    // through the roots just made.
    unsafe {
        if key_is_string && key.id() != TAG_STRING {
            // A kept key holds its own text: it is compared a word at a
            // time, and keeps no other string alive.
            let flat = string::copy_flat(mutator, &raw const held[1]);
            store(
                flat,
                load(flat) | ((hash & STRING_HASH_MASK) as usize) << 32,
            );
            held[1] = flat;
            key_tag = TAG_STRING;
        }
        let (mut number, mut entries) = used((&raw const held[0]).read_volatile());
        if number == load(entries + ENTRIES_CAPACITY) {
            grow(mutator, &raw const held[0]);
            (number, entries) = used((&raw const held[0]).read_volatile());
        }
        let [map, key, payload] = (&raw const held).read_volatile();
        store_record(
            record(entries, number),
            Value {
                tag: key_tag,
                payload: key,
            },
            value.kept(payload),
        );
        let capacity = load(entries + ENTRIES_CAPACITY);
        write32(hash_slot(entries, capacity, number), hash);
        let index = load(map + MAP_INDEX);
        if index != 0 {
            index_insert(index, read32(map + MAP_MASK) as usize, hash, number);
        }
        write32(map + MAP_USED, number as u32 + 1);
        write32(map + MAP_LEN, read32(map + MAP_LEN) + 1);
        write32(map + MAP_EPOCH, read32(map + MAP_EPOCH).wrapping_add(1));
    }
    mutator.unroot(roots);
}

/// Set `key` to `value` in the map at `map`, and return the value it had.
/// May collect.
///
/// # Safety
///
/// `map` is a live map, `key` a value of its key type and `value` a value
/// of its value type.
pub unsafe fn set(mutator: &mut Mutator, map: usize, key: Value, value: Value) -> Option<Value> {
    let kind = KeyKind::of_value(key);
    let key = Value {
        tag: key.tag,
        payload: kind.canonical(key.payload),
    };
    // SAFETY: the caller's contract.
    unsafe {
        let hash = kind.hash(key.payload);
        if let Some(found) = find(map, kind, key.payload, hash) {
            let slot = record(load(map + MAP_ENTRIES), found.number) + ENTRY_VALUE;
            let before = Value::load(slot);
            store_value(slot, value);
            return Some(before);
        }
        insert_new(mutator, map, key, hash, value);
    }
    None
}

/// Take `key` out of the map at `map`, and return the value it had.
///
/// # Safety
///
/// `map` is a live map, and `key` a value of its key type.
pub unsafe fn delete(map: usize, key: Value) -> Option<Value> {
    let kind = KeyKind::of_value(key);
    let key = kind.canonical(key.payload);
    // SAFETY: the caller's contract and the layout.
    unsafe {
        let found = find(map, kind, key, kind.hash(key))?;
        let entries = load(map + MAP_ENTRIES);
        let at = record(entries, found.number);
        let value = Value::load(at + ENTRY_VALUE);
        store_record(at, Value::NULL, Value::NULL);
        write32(
            hash_slot(entries, load(entries + ENTRIES_CAPACITY), found.number),
            0,
        );
        if found.slot != 0 {
            // Not empty, so a search goes on past it, and no hash.
            store(found.slot, 1);
        }
        write32(map + MAP_LEN, read32(map + MAP_LEN) - 1);
        write32(map + MAP_EPOCH, read32(map + MAP_EPOCH).wrapping_add(1));
        Some(value)
    }
}

/// Take every key out of the map at `map`.
///
/// # Safety
///
/// `map` is a live map.
pub unsafe fn clear(map: usize) {
    // SAFETY: the caller's contract. The entries it had are left to the
    // collector.
    unsafe {
        store(map + MAP_LEN, 0);
        store(map + MAP_ENTRIES, empty_entries());
        store(map + MAP_INDEX, 0);
        write32(map + MAP_MASK, u32::MAX);
        write32(map + MAP_EPOCH, read32(map + MAP_EPOCH).wrapping_add(1));
    }
}

/// Call `each` with the key and the value of every record of the map at
/// `map`, in the order the keys were first set.
///
/// # Safety
///
/// `map` is a live map, and `each` neither allocates nor changes it.
pub unsafe fn for_each(map: usize, mut each: impl FnMut(Value, Value)) {
    // SAFETY: the caller's contract and the layout.
    unsafe {
        let (used, entries) = used(map);
        let capacity = load(entries + ENTRIES_CAPACITY);
        for number in 0..used {
            if read32(hash_slot(entries, capacity, number)) != 0 {
                let at = record(entries, number);
                each(Value::load(at + ENTRY_KEY), Value::load(at + ENTRY_VALUE));
            }
        }
    }
}

crate::stubs::anchored! {
    /// The symbol of [`map_new`].
    baml_rt_map_new => map_new
}

/// A new map with room for `count` keys.
///
/// # Safety
///
/// Called from compiled code with its mutator.
unsafe extern "C" fn map_new(mutator: &mut Mutator, count: usize) -> usize {
    new(mutator, count)
}

crate::stubs::anchored! {
    /// The symbol of [`map_set`].
    baml_rt_map_set => map_set
}

/// `map[key] = value`, when the key may be there already. The key and the
/// value are each a tag and a payload.
///
/// # Safety
///
/// Called from compiled code with its mutator, a live map, and a key and
/// a value of the map's types.
unsafe extern "C" fn map_set(
    mutator: &mut Mutator,
    map: usize,
    key_tag: usize,
    key_payload: usize,
    value_tag: usize,
    value_payload: usize,
) {
    let key = Value {
        tag: key_tag,
        payload: key_payload,
    };
    let value = Value {
        tag: value_tag,
        payload: value_payload,
    };
    // SAFETY: the caller's contract.
    unsafe {
        set(mutator, map, key, value);
    }
}

crate::stubs::anchored! {
    /// The symbol of [`map_insert_new`].
    baml_rt_map_insert_new => map_insert_new
}

/// `map[key] = value`, after a probe did not find the key.
///
/// # Safety
///
/// As [`baml_rt_map_set`], and the map does not have the key.
unsafe extern "C" fn map_insert_new(
    mutator: &mut Mutator,
    map: usize,
    key_tag: usize,
    key_payload: usize,
    value_tag: usize,
    value_payload: usize,
) {
    let value = Value {
        tag: value_tag,
        payload: value_payload,
    };
    let kind = KeyKind::of_value(Value {
        tag: key_tag,
        payload: key_payload,
    });
    let key = Value {
        tag: key_tag,
        payload: kind.canonical(key_payload),
    };
    // SAFETY: the caller's contract.
    unsafe {
        let hash = kind.hash(key.payload);
        insert_new(mutator, map, key, hash, value);
    }
}

/// The address of the value of the key of tag `key_tag` and payload
/// `key_payload` in a map of keys of any kind, a slot, or zero. A leaf: it
/// allocates nothing.
///
/// # Safety
///
/// Called from compiled code with a live map and a key of its key type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_map_find(map: usize, key_tag: usize, key_payload: usize) -> usize {
    let key = Value {
        tag: key_tag,
        payload: key_payload,
    };
    // SAFETY: the caller's contract.
    unsafe { get(map, key).unwrap_or(0) }
}

/// Whether two maps have the same keys with equal values, in whatever
/// order. Two values are equal when both are strings of the same text,
/// both are floats that compare equal, or they have the same tag and the
/// same payload: two variants of an enum are equal when they are the same
/// object.
///
/// # Safety
///
/// Called from compiled code with two live maps of the same types.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_map_eq(left: usize, right: usize) -> u8 {
    // SAFETY: the caller's contract.
    unsafe {
        if left == right {
            return 1;
        }
        if len(left) != len(right) {
            return 0;
        }
        let mut equal = true;
        for_each(left, |key, value| {
            equal = equal && get(right, key).is_some_and(|slot| value_eq(value, Value::load(slot)));
        });
        u8::from(equal)
    }
}

/// Whether two values of one map's value type are equal, as
/// [`baml_rt_map_eq`] has it.
///
/// # Safety
///
/// Each is a value as a slot holds one.
unsafe fn value_eq(value: Value, other: Value) -> bool {
    let is_string = |value: Value| value.is_pointer() && value.id() < STRING_TAGS_END;
    // SAFETY: the caller's contract: a pointer is to a live object.
    unsafe {
        if is_string(value) && is_string(other) {
            return string::baml_rt_string_eq(value.payload, other.payload) != 0;
        }
        if value.id() != other.id() {
            return false;
        }
        if value.id() == TAG_FLOAT {
            return f64::from_bits(value.payload as u64) == f64::from_bits(other.payload as u64);
        }
        value.payload == other.payload
    }
}

/// What the string probe calls for a key that is a slice.
unsafe extern "C" fn find_str_slow(map: usize, key: usize, _hash: u32) -> usize {
    // SAFETY: the probe's caller passed a live map and string.
    unsafe { slot_of(map, KeyKind::Str, key).unwrap_or(0) }
}

// The leaf probes. They use no stack and only registers a call may
// clobber, and read the mutator's register for the seed. Each refers to
// nothing outside itself by a local label: a linker that drops unused
// functions moves the rest.
//
// `baml_rt_map_find_str(map, key, hash)`: `hash` is the key's cached hash,
// not zero. Returns the address of the key's value, a slot, or zero. A
// record is two slots, so record `n` is at `16 + 32 * n` of the entries
// and the hashes of entries with no index are at `16 + 32 * 8`.
// `baml_rt_map_find_int(map, key)`: the same for a key that is its bits.
// A probe reads of a record only the payload of its key, whose type id the
// kind of the map fixes: one atomic load (`ldar`), no busy bit. What it
// returns is a slot's address, which the caller reads or writes as a slot.
// `baml_rt_str_hash(string)`: computes a string's hash, caches it in its
// header and returns it.
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    ".text",
    ".p2align 4",
    ".globl {find_str}",
    "{find_str}:",
    "ldr   w16, [x1]",
    "cmp   w16, #{string}",
    "b.ne  24f",
    "orr   w2, w2, #0x80000000",
    "ldp   x3, x4, [x0, #{entries}]",
    "ldr   x5, [x1, #8]",
    "2:",
    "cbz   x4, 7f",
    "ldr   w6, [x0, #{mask}]",
    "add   x4, x4, #16",
    "and   w7, w2, w6",
    "3:",
    "ldr   x8, [x4, x7, lsl #3]",
    "cmp   w8, w2",
    "b.eq  5f",
    "4:",
    "cbz   x8, 9f",
    "add   w7, w7, #1",
    "and   w7, w7, w6",
    "b     3b",
    "5:",
    "lsr   x9, x8, #32",
    "add   x9, x3, x9, lsl #5",
    // Compare the key of the record whose value is at x9: the same
    // object, or the same lengths and then the same words of text.
    "6:",
    "sub   x10, x9, #8",
    "ldar  x10, [x10]",
    "cmp   x10, x1",
    "b.eq  20f",
    "ldr   x11, [x10, #8]",
    "cmp   x11, x5",
    "b.ne  21f",
    "cbz   w5, 20f",
    "ldr   x12, [x10, #16]",
    "ldr   x13, [x1, #16]",
    "cmp   x12, x13",
    "b.ne  21f",
    "cmp   w5, #8",
    "b.ls  20f",
    "ldr   x12, [x10, #24]",
    "ldr   x13, [x1, #24]",
    "cmp   x12, x13",
    "b.ne  21f",
    "cmp   w5, #16",
    "b.hi  22f",
    "20:",
    "mov   x0, x9",
    "ret",
    "21:",
    "cbnz  x4, 4b",
    // No index: clear the byte of the record just tried, take the next.
    "sub   x14, x9, x3",
    "lsr   x14, x14, #2",
    "sub   x14, x14, #8",
    "mov   x15, #0xff",
    "lsl   x15, x15, x14",
    "bic   x8, x8, x15",
    "cbnz  x8, 8f",
    "9:",
    "mov   x0, #0",
    "ret",
    // No index: the eight hashes after the eight records, at once.
    "7:",
    "ldp   q0, q1, [x3, #{hashes}]",
    "dup   v2.4s, w2",
    "cmeq  v0.4s, v0.4s, v2.4s",
    "cmeq  v1.4s, v1.4s, v2.4s",
    "uzp1  v0.8h, v0.8h, v1.8h",
    "xtn   v0.8b, v0.8h",
    "fmov  x8, d0",
    "cbz   x8, 9b",
    "8:",
    "rbit  x9, x8",
    "clz   x9, x9",
    "add   x9, x3, x9, lsl #2",
    "add   x9, x9, #32",
    "b     6b",
    // More than two words of text.
    "22:",
    "mov   w14, w5",
    "add   x14, x14, #7",
    "lsr   x14, x14, #3",
    "mov   x15, #2",
    "23:",
    "add   x11, x10, x15, lsl #3",
    "add   x12, x1, x15, lsl #3",
    "ldr   x11, [x11, #16]",
    "ldr   x12, [x12, #16]",
    "cmp   x11, x12",
    "b.ne  21b",
    "add   x15, x15, #1",
    "cmp   x15, x14",
    "b.lo  23b",
    "b     20b",
    "24:",
    "b     {find_str_slow}",
    "",
    ".p2align 4",
    ".globl {find_int}",
    "{find_int}:",
    "ldr   x16, [x21, #{seed}]",
    "ldr   x17, 30f",
    "eor   x16, x16, x1",
    "mul   x2, x16, x17",
    "umulh x16, x16, x17",
    "eor   x2, x2, x16",
    "lsr   x2, x2, #32",
    "orr   w2, w2, #0x80000000",
    "ldp   x3, x4, [x0, #{entries}]",
    "cbz   x4, 36f",
    "ldr   w6, [x0, #{mask}]",
    "add   x4, x4, #16",
    "and   w7, w2, w6",
    "32:",
    "ldr   x8, [x4, x7, lsl #3]",
    "cmp   w8, w2",
    "b.eq  34f",
    "33:",
    "cbz   x8, 39f",
    "add   w7, w7, #1",
    "and   w7, w7, w6",
    "b     32b",
    "34:",
    "lsr   x9, x8, #32",
    "add   x9, x3, x9, lsl #5",
    "sub   x10, x9, #8",
    "ldar  x10, [x10]",
    "cmp   x10, x1",
    "b.ne  33b",
    "mov   x0, x9",
    "ret",
    "36:",
    "ldp   q0, q1, [x3, #{hashes}]",
    "dup   v2.4s, w2",
    "cmeq  v0.4s, v0.4s, v2.4s",
    "cmeq  v1.4s, v1.4s, v2.4s",
    "uzp1  v0.8h, v0.8h, v1.8h",
    "xtn   v0.8b, v0.8h",
    "fmov  x8, d0",
    "37:",
    "cbz   x8, 39f",
    "rbit  x9, x8",
    "clz   x12, x9",
    "add   x9, x3, x12, lsl #2",
    "add   x10, x9, #24",
    "ldar  x10, [x10]",
    "cmp   x10, x1",
    "b.eq  38f",
    "mov   x11, #0xff",
    "lsl   x11, x11, x12",
    "bic   x8, x8, x11",
    "b     37b",
    "38:",
    "add   x0, x9, #32",
    "ret",
    "39:",
    "mov   x0, #0",
    "ret",
    ".p2align 3",
    "30:",
    ".quad {s1}",
    "",
    ".p2align 4",
    ".globl {str_hash}",
    "{str_hash}:",
    "ldr   w16, [x0]",
    "cmp   w16, #{string}",
    "b.ne  46f",
    "ldr   w1, [x0, #8]",
    "ldr   x2, [x21, #{seed}]",
    "ldr   x3, 51f",
    "ldr   x4, 50f",
    "mov   x5, #0",
    "mov   x6, #0",
    "cbz   w1, 42f",
    "cmp   w1, #16",
    "b.hi  43f",
    "ldr   x5, [x0, #16]",
    "cmp   w1, #8",
    "b.ls  42f",
    "ldr   x6, [x0, #24]",
    "42:",
    "eor   x5, x5, x3",
    "eor   x6, x6, x2",
    "mul   x7, x5, x6",
    "umulh x8, x5, x6",
    "eor   x7, x7, x8",
    "eor   x4, x4, x1",
    "mul   x5, x4, x7",
    "umulh x6, x4, x7",
    "eor   x5, x5, x6",
    "lsr   x5, x5, #32",
    "ands  w5, w5, #{hash_mask}",
    "csinc w5, w5, wzr, ne",
    "ldr   x6, [x0]",
    "orr   x6, x6, x5, lsl #32",
    "str   x6, [x0]",
    "mov   w0, w5",
    "ret",
    // More than two words: fold them two at a time into the seed, and
    // finish with the one or two left.
    "43:",
    "add   x9, x1, #7",
    "lsr   x9, x9, #3",
    "add   x10, x0, #16",
    "mov   x11, #0",
    "44:",
    "add   x12, x11, #2",
    "cmp   x12, x9",
    "b.hs  45f",
    "add   x15, x10, x11, lsl #3",
    "ldp   x13, x14, [x15]",
    "eor   x13, x13, x3",
    "eor   x14, x14, x2",
    "mul   x15, x13, x14",
    "umulh x2, x13, x14",
    "eor   x2, x2, x15",
    "mov   x11, x12",
    "b     44b",
    "45:",
    "ldr   x5, [x10, x11, lsl #3]",
    "add   x12, x11, #1",
    "cmp   x12, x9",
    "b.hs  42b",
    "ldr   x6, [x10, x12, lsl #3]",
    "b     42b",
    "46:",
    "b     {str_hash_slow}",
    ".p2align 3",
    "50:",
    ".quad {s2}",
    "51:",
    ".quad {s1}",
    find_str = sym baml_rt_map_find_str,
    find_int = sym baml_rt_map_find_int,
    str_hash = sym baml_rt_str_hash,
    find_str_slow = sym find_str_slow,
    str_hash_slow = sym str_hash_slow,
    string = const TAG_STRING,
    hashes = const ENTRIES_RECORDS + SMALL_ENTRIES * ENTRY_SIZE,
    hash_mask = const STRING_HASH_MASK,
    entries = const MAP_ENTRIES,
    mask = const MAP_MASK,
    seed = const crate::layout::MUTATOR_HASH_SEED,
    s1 = const S1,
    s2 = const S2,
);

#[cfg(target_arch = "aarch64")]
unsafe extern "C" {
    /// The address of the value of the string `key`, whose cached hash is
    /// `hash`, in `map`, or zero.
    pub fn baml_rt_map_find_str(map: usize, key: usize, hash: u32) -> usize;
    /// The address of the value of `key` in `map`, or zero.
    pub fn baml_rt_map_find_int(map: usize, key: usize) -> usize;
    /// The hash of `string`, which it caches in the string's header.
    pub fn baml_rt_str_hash(string: usize) -> u32;
}

/// The probes where there is no assembly for them. Not built or run:
/// written on an arm64 host.
#[cfg(not(target_arch = "aarch64"))]
mod portable {
    use super::{KeyKind, slot_of, str_hash};

    /// # Safety
    ///
    /// Called from compiled code with a live map of string keys and one.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn baml_rt_map_find_str(map: usize, key: usize, _hash: u32) -> usize {
        // SAFETY: the caller's contract.
        unsafe { slot_of(map, KeyKind::Str, key).unwrap_or(0) }
    }

    /// # Safety
    ///
    /// Called from compiled code with a live map of keys that are their
    /// bits and one.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn baml_rt_map_find_int(map: usize, key: usize) -> usize {
        // SAFETY: the caller's contract.
        unsafe { slot_of(map, KeyKind::Word, key).unwrap_or(0) }
    }

    /// # Safety
    ///
    /// Called from compiled code with a live string.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn baml_rt_str_hash(string: usize) -> u32 {
        // SAFETY: the caller's contract.
        unsafe { str_hash(string) }
    }
}
#[cfg(not(target_arch = "aarch64"))]
pub use portable::*;

#[cfg(test)]
mod tests;
