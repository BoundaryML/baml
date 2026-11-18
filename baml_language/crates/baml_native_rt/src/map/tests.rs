use super::*;
use crate::{
    heap::Program,
    layout::{
        RUNTIME_TYPES, SLICE_OFFSET, SLICE_PARENT, SLICE_SIZE, TYPE_BOX_INT, TYPE_STRING_SLICE,
    },
};

fn mutator(stress: bool) -> Mutator {
    let program = Program {
        stack_maps: crate::tests::NO_STACK_MAPS.as_ptr() as usize,
        types: RUNTIME_TYPES.as_ptr() as usize,
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
unsafe fn find_str(mutator: &Mutator, map: usize, key: usize) -> Option<usize> {
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
        (slot != 0).then(|| load(slot))
    }
}

unsafe fn find_int(mutator: &Mutator, map: usize, key: usize) -> Option<usize> {
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
        (slot != 0).then(|| load(slot))
    }
}

fn slice(mutator: &mut Mutator, parent: &mut usize, from: usize, bytes: usize) -> usize {
    mutator.root(parent);
    let slice = mutator.allocate_object(TYPE_STRING_SLICE, SLICE_SIZE);
    mutator.unroot(1);
    // SAFETY: the slice has room for its four words.
    unsafe {
        store(slice + STRING_LENGTHS, bytes | bytes << 32);
        store(slice + SLICE_PARENT, *parent);
        store(slice + SLICE_OFFSET, from);
    }
    slice
}

fn keys(map: usize) -> Vec<String> {
    let mut keys = Vec::new();
    // SAFETY: a live map of string keys.
    unsafe { for_each(map, |key, _| keys.push(string::text(key).to_owned())) };
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
    let mut map = new(&mut mutator, true, true, 0);
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
            assert_eq!(set(&mut mutator, map, KeyKind::Str, true, key, value), None);
        }
        assert_eq!(len(map), 300);
        assert_eq!(keys(map), (0..300).map(name).collect::<Vec<_>>());
        for number in (0..300).filter(|number| number % 2 == 0) {
            let key = string::from_str(&mut mutator, &name(number));
            let value = delete(map, KeyKind::Str, key).expect("the key is there");
            assert_eq!(string::text(value), format!("value {number}"));
            assert_eq!(delete(map, KeyKind::Str, key), None);
        }
        // Setting a deleted key again puts it last; one still there stays.
        for number in [4, 7] {
            let mut key = string::from_str(&mut mutator, &name(number));
            mutator.root(&mut key);
            let value = string::from_str(&mut mutator, "again");
            mutator.unroot(1);
            let before = set(&mut mutator, map, KeyKind::Str, true, key, value);
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
            set(&mut mutator, map, KeyKind::Str, true, key, value);
        }
        expected.extend((300..700).map(name));
        assert_eq!(keys(map), expected);
        assert_eq!(len(map), expected.len());
        for number in 0..700 {
            let mut key = string::from_str(&mut mutator, &name(number));
            let there = number == 4 || number >= 300 || number % 2 == 1;
            let found = get(map, KeyKind::Str, key).map(|slot| load(slot));
            assert_eq!(found.is_some(), there, "{number}");
            assert_eq!(find_str(&mutator, map, key), found, "{number}");
            if let Some(value) = found {
                let text = if number == 4 || number == 7 {
                    "again".to_owned()
                } else {
                    format!("value {number}")
                };
                assert_eq!(string::text(value), text);
            }
            // A slice finds what the string it is of the text of does.
            if name(number).len() >= crate::layout::SLICE_MIN_BYTES {
                key = string::from_str(&mut mutator, &format!("ab{}", name(number)));
                let part = slice(&mut mutator, &mut key, 2, name(number).len());
                let found = get(map, KeyKind::Str, part).map(|slot| load(slot));
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
    let mut map = new(&mut mutator, true, false, 0);
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
            set(&mut mutator, map, KeyKind::Str, false, key, number);
            for (probed, text) in texts.iter().enumerate() {
                // Another string of the same text, its hash not yet known.
                let key = string::from_str(&mut mutator, text);
                let found = find_str(&mutator, map, key);
                assert_eq!(found, (probed <= number).then_some(probed), "{text}");
            }
        }
        let missing = string::from_str(&mut mutator, "k3");
        assert_eq!(find_str(&mutator, map, missing), None);
        delete(map, KeyKind::Str, missing);
        let gone = string::from_str(&mut mutator, "k1");
        assert_eq!(delete(map, KeyKind::Str, gone), Some(2));
        assert_eq!(find_str(&mutator, map, gone), None);
    }
    mutator.unroot(1);
}

#[test]
fn word_keys_are_found_by_their_bits() {
    let mut mutator = mutator(true);
    let mut map = new(&mut mutator, false, false, 3);
    mutator.root(&mut map);
    let key = |number: usize| number.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (number >> 3);
    // SAFETY: `map` is a rooted live map of int keys and values.
    unsafe {
        for number in 0..5000 {
            set(&mut mutator, map, KeyKind::Word, false, key(number), number);
            if number % 5 == 0 {
                assert_eq!(delete(map, KeyKind::Word, key(number)), Some(number));
            }
            if number == 6 || number == 2000 {
                for probed in 0..number + 3 {
                    let there = probed <= number && probed % 5 != 0;
                    assert_eq!(
                        find_int(&mutator, map, key(probed)),
                        there.then_some(probed)
                    );
                }
            }
        }
        assert_eq!(len(map), 4000);
        for number in 0..5000 {
            let found = get(map, KeyKind::Word, key(number)).map(|slot| load(slot));
            assert_eq!(found, (number % 5 != 0).then_some(number));
            assert_eq!(find_int(&mutator, map, key(number)), found);
        }
        // A float's zeros are one key, and so is every NaN.
        let mut floats = new(&mut mutator, false, false, 0);
        mutator.root(&mut floats);
        let kind = KeyKind::of(crate::layout::TYPE_BOX_FLOAT);
        assert_eq!(KeyKind::of(TYPE_BOX_INT), KeyKind::Word);
        set(
            &mut mutator,
            floats,
            kind,
            false,
            0.0f64.to_bits() as usize,
            1,
        );
        let negative = (-0.0f64).to_bits() as usize;
        assert_eq!(set(&mut mutator, floats, kind, false, negative, 2), Some(1));
        set(
            &mut mutator,
            floats,
            kind,
            false,
            f64::NAN.to_bits() as usize,
            3,
        );
        let other_nan = (f64::NAN.to_bits() | 1) as usize;
        assert_eq!(get(floats, kind, other_nan).map(|slot| load(slot)), Some(3));
    }
    mutator.unroot(2);
}
