use super::*;
use crate::{
    heap::Program,
    layout::{
        SLICE_OFFSET, SLICE_PARENT, SLICE_SIZE, TAG_STRING_SLICE, VALUE, VARIANT_ENUM,
        VARIANT_NAME, VARIANT_SIZE,
    },
};

fn mutator(stress: bool) -> Mutator {
    let program = Program {
        stack_maps: crate::tests::NO_STACK_MAPS.as_ptr() as usize,
        types: crate::tests::NO_TYPES.as_ptr() as usize,
        unwind: 0,
        functions: 0,
    };
    // SAFETY: both tables are in the compiler's formats.
    unsafe { Mutator::new(program, stress) }
}

/// Call a probe as compiled code does: with the mutator in its register.
#[cfg(target_arch = "aarch64")]
unsafe fn probe(
    mutator: &Mutator,
    function: usize,
    first: usize,
    second: usize,
    third: usize,
) -> usize {
    let result: usize;
    // SAFETY: the caller passes a probe and arguments it takes.
    unsafe {
        core::arch::asm!(
            "blr {function}",
            function = in(reg) function,
            inout("x0") first => result,
            in("x1") second,
            in("x2") third,
            inout("x21") std::ptr::from_ref(mutator) as usize => _,
            clobber_abi("C"),
        );
    }
    result
}

/// The value of a string key as compiled code finds it.
unsafe fn find_str(mutator: &Mutator, map: usize, key: usize) -> Option<Value> {
    // SAFETY: the caller passes a live map and string.
    unsafe {
        #[cfg(target_arch = "aarch64")]
        let slot = {
            let hash = probe(mutator, baml_rt_str_hash as *const () as usize, key, 0, 0);
            probe(
                mutator,
                baml_rt_map_find_str as *const () as usize,
                map,
                key,
                hash,
            )
        };
        #[cfg(not(target_arch = "aarch64"))]
        let slot = baml_rt_map_find_str(map, key, str_hash(key));
        let _ = mutator;
        (slot != 0).then(|| Value::load(slot))
    }
}

unsafe fn find_int(mutator: &Mutator, map: usize, key: usize) -> Option<Value> {
    // SAFETY: the caller passes a live map.
    unsafe {
        #[cfg(target_arch = "aarch64")]
        let slot = probe(
            mutator,
            baml_rt_map_find_int as *const () as usize,
            map,
            key,
            0,
        );
        #[cfg(not(target_arch = "aarch64"))]
        let slot = baml_rt_map_find_int(map, key);
        let _ = mutator;
        (slot != 0).then(|| Value::load(slot))
    }
}

fn slice(mutator: &mut Mutator, parent: &mut usize, from: usize, bytes: usize) -> usize {
    mutator.root(parent);
    let slice = mutator.allocate_object(TAG_STRING_SLICE, SLICE_SIZE);
    mutator.unroot(1);
    // SAFETY: the slice has room for its four words.
    unsafe {
        store(slice + STRING_LENGTHS, bytes | bytes << 32);
        store(slice + SLICE_PARENT, *parent);
        store(slice + SLICE_OFFSET, from);
    }
    slice
}

/// The `int` a test keeps under a key.
fn int(value: usize) -> Value {
    Value::int(value.cast_signed() as i64)
}

fn keys(map: usize) -> Vec<String> {
    let mut keys = Vec::new();
    // SAFETY: a live map of string keys.
    unsafe {
        for_each(map, |key, _| {
            assert_eq!(key.tag, TAG_STRING, "a kept key holds its own text");
            keys.push(string::text(key.payload).to_owned());
        });
    }
    keys
}

#[test]
fn a_string_hashes_the_same_however_it_is_held() {
    let mut mutator = mutator(false);
    let text = "abcdefghijklmnopqrstuvwxyz0123456789-abcdefghijklmnopqrstuvwxyz";
    for length in 0..text.len() {
        let expected = text_hash(&text.as_bytes()[..length]);
        assert!(expected != 0 && expected & !STRING_HASH_MASK == 0);
        let flat = string::from_str(&mut mutator, &text[..length]);
        // SAFETY: live strings.
        unsafe {
            #[cfg(target_arch = "aarch64")]
            assert_eq!(
                probe(&mutator, baml_rt_str_hash as *const () as usize, flat, 0, 0),
                expected as usize
            );
            assert_eq!(str_hash(flat), expected);
            assert_eq!(load(flat) >> 32, expected as usize);
            if length >= crate::layout::SLICE_MIN_BYTES {
                let mut whole = string::from_str(&mut mutator, &format!("xyz{text}"));
                let part = slice(&mut mutator, &mut whole, 3, length);
                #[cfg(target_arch = "aarch64")]
                assert_eq!(
                    probe(&mutator, baml_rt_str_hash as *const () as usize, part, 0, 0),
                    expected as usize
                );
                assert_eq!(str_hash(part), expected);
            }
        }
    }
}

#[test]
fn string_keys_keep_their_order_through_growth_deletes_and_collections() {
    let mut mutator = mutator(true);
    let mut map = new(&mut mutator, 0);
    mutator.root(&mut map);
    let name = |number: usize| format!("key-{number}-{}", "x".repeat(number % 23));
    // SAFETY: `map` is a rooted live map of string keys and values.
    unsafe {
        for number in 0..300 {
            // Every third key arrives as a slice of a longer string.
            let text = name(number);
            let mut key = if number % 3 == 0 && text.len() >= crate::layout::SLICE_MIN_BYTES {
                let mut whole = string::from_str(&mut mutator, &format!("__{text}__"));
                slice(&mut mutator, &mut whole, 2, text.len())
            } else {
                string::from_str(&mut mutator, &text)
            };
            mutator.root(&mut key);
            let value = string::from_str(&mut mutator, &format!("value {number}"));
            mutator.unroot(1);
            let (key, value) = (Value::reference(key), Value::reference(value));
            assert_eq!(set(&mut mutator, map, key, value), None);
        }
        assert_eq!(len(map), 300);
        assert_eq!(keys(map), (0..300).map(name).collect::<Vec<_>>());
        for number in (0..300).filter(|number| number % 2 == 0) {
            let key = Value::reference(string::from_str(&mut mutator, &name(number)));
            let value = delete(map, key).expect("the key is there");
            assert_eq!(value.tag, TAG_STRING);
            assert_eq!(string::text(value.payload), format!("value {number}"));
            assert_eq!(delete(map, key), None);
        }
        // Setting a deleted key again puts it last; one still there stays.
        for number in [4, 7] {
            let mut key = string::from_str(&mut mutator, &name(number));
            mutator.root(&mut key);
            let value = string::from_str(&mut mutator, "again");
            mutator.unroot(1);
            let (key, value) = (Value::reference(key), Value::reference(value));
            let before = set(&mut mutator, map, key, value);
            assert_eq!(before.is_some(), number == 7);
        }
        let mut expected: Vec<_> = (0..300)
            .filter(|number| number % 2 == 1)
            .map(name)
            .collect();
        expected.push(name(4));
        assert_eq!(keys(map), expected);
        // Enough new keys to use the records up: the dead ones go.
        for number in 300..700 {
            let mut key = string::from_str(&mut mutator, &name(number));
            mutator.root(&mut key);
            let value = string::from_str(&mut mutator, &format!("value {number}"));
            mutator.unroot(1);
            let (key, value) = (Value::reference(key), Value::reference(value));
            set(&mut mutator, map, key, value);
        }
        expected.extend((300..700).map(name));
        assert_eq!(keys(map), expected);
        assert_eq!(len(map), expected.len());
        for number in 0..700 {
            let mut key = string::from_str(&mut mutator, &name(number));
            let there = number == 4 || number >= 300 || number % 2 == 1;
            let found = get(map, Value::reference(key)).map(|slot| Value::load(slot));
            assert_eq!(found.is_some(), there, "{number}");
            assert_eq!(find_str(&mutator, map, key), found, "{number}");
            if let Some(value) = found {
                let text = if number == 4 || number == 7 {
                    "again".to_owned()
                } else {
                    format!("value {number}")
                };
                assert_eq!(string::text(value.payload), text);
            }
            // A slice finds what the string it is of the text of does.
            if name(number).len() >= crate::layout::SLICE_MIN_BYTES {
                key = string::from_str(&mut mutator, &format!("ab{}", name(number)));
                let part = slice(&mut mutator, &mut key, 2, name(number).len());
                let found = get(map, Value::reference(part)).map(|slot| Value::load(slot));
                assert_eq!(found.is_some(), there);
                assert_eq!(find_str(&mutator, map, part), found);
            }
        }
        clear(map);
        assert_eq!(len(map), 0);
        let key = string::from_str(&mut mutator, &name(301));
        assert_eq!(find_str(&mutator, map, key), None);
    }
    mutator.unroot(1);
}

#[test]
fn a_few_string_keys_are_found_without_an_index() {
    let mut mutator = mutator(false);
    let mut map = new(&mut mutator, 0);
    mutator.root(&mut map);
    let texts = [
        "",
        "k",
        "k1",
        "eight888",
        "nine99999",
        "seventeen-bytes-17",
        "k2",
    ];
    // SAFETY: `map` is a rooted live map of string keys.
    unsafe {
        for (number, text) in texts.iter().enumerate() {
            let key = string::from_str(&mut mutator, text);
            set(&mut mutator, map, Value::reference(key), int(number));
            for (probed, text) in texts.iter().enumerate() {
                // Another string of the same text, its hash not yet known.
                let key = string::from_str(&mut mutator, text);
                let found = find_str(&mutator, map, key);
                assert_eq!(found, (probed <= number).then_some(int(probed)), "{text}");
            }
        }
        let missing = string::from_str(&mut mutator, "k3");
        assert_eq!(find_str(&mutator, map, missing), None);
        delete(map, Value::reference(missing));
        let gone = string::from_str(&mut mutator, "k1");
        assert_eq!(delete(map, Value::reference(gone)), Some(int(2)));
        assert_eq!(find_str(&mutator, map, gone), None);
    }
    mutator.unroot(1);
}

#[test]
fn word_keys_are_found_by_their_bits() {
    let mut mutator = mutator(true);
    let mut map = new(&mut mutator, 3);
    mutator.root(&mut map);
    let key = |number: usize| number.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (number >> 3);
    // SAFETY: `map` is a rooted live map of int keys and values.
    unsafe {
        for number in 0..5000 {
            set(&mut mutator, map, int(key(number)), int(number));
            if number % 5 == 0 {
                assert_eq!(delete(map, int(key(number))), Some(int(number)));
            }
            if number == 6 || number == 2000 {
                for probed in 0..number + 3 {
                    let there = probed <= number && probed % 5 != 0;
                    assert_eq!(
                        find_int(&mutator, map, key(probed)),
                        there.then_some(int(probed))
                    );
                }
            }
        }
        assert_eq!(len(map), 4000);
        for number in 0..5000 {
            let found = get(map, int(key(number))).map(|slot| Value::load(slot));
            assert_eq!(found, (number % 5 != 0).then_some(int(number)));
            assert_eq!(find_int(&mutator, map, key(number)), found);
        }
        // A float's zeros are one key, and so is every NaN.
        let mut floats = new(&mut mutator, 0);
        mutator.root(&mut floats);
        assert_eq!(KeyKind::of_value(Value::float(1.5)), KeyKind::Float);
        assert_eq!(KeyKind::of_value(int(1)), KeyKind::Word);
        set(&mut mutator, floats, Value::float(0.0), int(1));
        assert_eq!(
            set(&mut mutator, floats, Value::float(-0.0), int(2)),
            Some(int(1))
        );
        set(&mut mutator, floats, Value::float(f64::NAN), int(3));
        let other_nan = Value::float(f64::from_bits(f64::NAN.to_bits() | 1));
        assert_eq!(
            get(floats, other_nan).map(|slot| Value::load(slot)),
            Some(int(3))
        );
    }
    mutator.unroot(2);
}

/// A key that is an enum's variant is found by its address, wherever a
/// collection has moved its object since it was set: its hash is of its
/// position. Two variants at one position, of two enums, are two keys.
#[test]
fn variant_keys_are_found_after_their_objects_move() {
    const VARIANTS: usize = 40;
    let mut mutator = mutator(true);
    let mut map = new(&mut mutator, 0);
    mutator.root(&mut map);
    // Two variants at each position, in the heap, each a root.
    let mut variants = [0_usize; 2 * VARIANTS];
    let held = variants.as_mut_ptr();
    // SAFETY: each variant is laid out before the next allocation and read
    // back from its root after one, and `map` is a rooted live map.
    unsafe {
        for number in 0..2 * VARIANTS {
            mutator.root_at(held.add(number) as usize);
            let variant = mutator.allocate_object(TAG_ENUM_VARIANT, VARIANT_SIZE);
            store(variant + VARIANT_ENUM, 0);
            store(variant + VARIANT_INDEX, number / 2);
            store(variant + VARIANT_NAME, 0);
            held.add(number).write(variant);
        }
        let key = |number: usize| Value::reference(held.add(number).read());
        assert_eq!(KeyKind::of_value(key(0)), KeyKind::Variant);
        for number in 0..2 * VARIANTS {
            assert_eq!(set(&mut mutator, map, key(number), int(number)), None);
        }
        // The last key was set where its object was then, and the first
        // many collections ago.
        let before = held.add(2 * VARIANTS - 1).read();
        mutator.allocate(VALUE);
        assert_ne!(held.add(2 * VARIANTS - 1).read(), before);
        assert_eq!(len(map), 2 * VARIANTS);
        for number in 0..2 * VARIANTS {
            let slot = baml_rt_map_find(map, TAG_ENUM_VARIANT, held.add(number).read());
            assert_ne!(slot, 0);
            assert_eq!(Value::load(slot), int(number));
        }
        assert_eq!(delete(map, key(3)), Some(int(3)));
        assert_eq!(
            baml_rt_map_find(map, TAG_ENUM_VARIANT, held.add(3).read()),
            0
        );
        assert_eq!(get(map, key(2)).map(|slot| Value::load(slot)), Some(int(2)));
    }
    mutator.unroot(1 + 2 * VARIANTS);
}
