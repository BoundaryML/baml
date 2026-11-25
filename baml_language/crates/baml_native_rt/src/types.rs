//! Type objects made when the program runs.
//!
//! Code compiled once for every type it is called with is passed the type
//! objects of its type parameters. Where it names a type made of them
//! (`T?`, `T | string`), it asks here for that type's object. The same
//! type always gives the same object, whatever order its arguments are
//! given in and however it was first met: the objects the compiler placed
//! for the types the program names whole are in the table before anything
//! is asked ([`PROGRAM_BUILT_TYPES`]).
//!
//! An object made here is outside the heap: the collector leaves its
//! address alone wherever it finds it, and it is never freed. Its
//! arguments are objects of the same kind or of the executable's data.

use std::{
    collections::HashMap,
    sync::{Mutex, PoisonError},
};

use crate::{
    heap::{Mutator, load, store},
    layout::{
        PROGRAM_BUILT_TYPES, TAG_TYPE, TYPE_ARGUMENTS, TYPE_CLASS, TYPE_COUNT, TYPE_KIND,
        TYPE_KIND_UNION, VALUE, WORD, type_size,
    },
};

/// What makes two types with arguments one type: their kind, the class
/// object they name and their arguments.
#[derive(PartialEq, Eq, Hash)]
struct Key {
    kind: u32,
    class: usize,
    arguments: Vec<usize>,
}

/// The type object of each type with arguments met so far, the program's
/// own first.
static TABLE: Mutex<Option<HashMap<Key, usize>>> = Mutex::new(None);

/// The kind of the type object at `ty`.
///
/// # Safety
///
/// `ty` is a live type object.
pub(crate) unsafe fn kind(ty: usize) -> u32 {
    // SAFETY: the caller's contract.
    unsafe { *((ty + TYPE_KIND) as *const u32) }
}

/// The arguments of the type object at `ty`.
///
/// # Safety
///
/// `ty` is a live type object that nothing moves while the slice is used.
pub(crate) unsafe fn arguments<'a>(ty: usize) -> &'a [usize] {
    // SAFETY: the caller's contract: the object has as many words of
    // arguments as its count says.
    unsafe {
        let count = *((ty + TYPE_COUNT) as *const u32) as usize;
        std::slice::from_raw_parts((ty + TYPE_ARGUMENTS) as *const usize, count)
    }
}

/// The arguments one type of `kind` has, however they were given: a
/// union's members with those of a member that is itself a union in its
/// place, in the order of their addresses, each once.
///
/// # Safety
///
/// Every argument is a live type object.
unsafe fn normalized(kind: u32, given: &[usize]) -> Vec<usize> {
    if kind != TYPE_KIND_UNION {
        return given.to_vec();
    }
    let mut members = Vec::with_capacity(given.len());
    for member in given {
        // SAFETY: the caller's contract.
        unsafe {
            if self::kind(*member) == TYPE_KIND_UNION {
                members.extend_from_slice(arguments(*member));
            } else {
                members.push(*member);
            }
        }
    }
    members.sort_unstable();
    members.dedup();
    members
}

/// The table with the program's own type objects in it.
///
/// # Safety
///
/// `types` is a program's table of types.
unsafe fn seeded(types: usize) -> HashMap<Key, usize> {
    let mut table = HashMap::new();
    // SAFETY: the caller's contract: the word is zero or the address of a
    // count and that many live type objects.
    unsafe {
        let list = load(types + PROGRAM_BUILT_TYPES);
        if list != 0 {
            for index in 0..load(list) {
                let ty = load(list + (1 + index) * WORD);
                let key = Key {
                    kind: kind(ty),
                    class: load(ty + TYPE_CLASS),
                    arguments: normalized(kind(ty), arguments(ty)),
                };
                // The first of two objects of one type stands for it.
                table.entry(key).or_insert(ty);
            }
        }
    }
    table
}

/// A new type object outside the heap.
fn made(key: &Key) -> usize {
    let size = type_size(key.arguments.len());
    let layout = std::alloc::Layout::from_size_align(size, VALUE).expect("a type object's size");
    // SAFETY: the size is not zero; the object is written within it.
    unsafe {
        let ty = std::alloc::alloc_zeroed(layout) as usize;
        assert!(ty != 0, "out of memory for a type object");
        store(ty, TAG_TYPE);
        store(ty + TYPE_CLASS, key.class);
        store(
            ty + TYPE_KIND,
            key.kind as usize | key.arguments.len() << 32,
        );
        for (index, argument) in key.arguments.iter().enumerate() {
            store(ty + TYPE_ARGUMENTS + index * WORD, *argument);
        }
        ty
    }
}

/// The type object of the type of `kind` that names the class object
/// `class`, or none, and has the `count` arguments at `arguments`: the one
/// object there is of that type, made the first time it is asked for. A
/// union of one member is that member. A leaf: it does not allocate in
/// the heap, and does not throw.
///
/// # Safety
///
/// Called from compiled code with its mutator and `count` words at
/// `arguments`, each the address of a live type object.
#[unsafe(no_mangle)]
unsafe extern "C" fn baml_rt_type_intern(
    worker: &Mutator,
    kind: usize,
    class: usize,
    arguments: *const usize,
    count: usize,
) -> usize {
    let kind = u32::try_from(kind).expect("a kind of type");
    // SAFETY: the caller's contract.
    let arguments = unsafe { normalized(kind, std::slice::from_raw_parts(arguments, count)) };
    if kind == TYPE_KIND_UNION
        && let [only] = arguments[..]
    {
        return only;
    }
    let key = Key {
        kind,
        class,
        arguments,
    };
    let mut table = TABLE.lock().unwrap_or_else(PoisonError::into_inner);
    // SAFETY: a mutator's table of types is its program's.
    let table = table.get_or_insert_with(|| unsafe { seeded(worker.types()) });
    if let Some(known) = table.get(&key) {
        return *known;
    }
    let ty = made(&key);
    table.insert(key, ty);
    ty
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        heap::Program,
        layout::{PROGRAM_DESCRIPTIONS, TYPE_KIND_NAMED},
    };

    /// A type object that is never freed, as one in a program's data.
    fn placed(kind: u32, arguments: &[usize]) -> usize {
        made(&Key {
            kind,
            class: 0,
            arguments: arguments.to_vec(),
        })
    }

    /// One type is one object: whatever order a union's members are given
    /// in, whether a member is itself a union, and whether the program
    /// placed the object or it was made when asked for.
    #[test]
    fn a_type_asked_for_twice_is_one_object() {
        let [a, b, c] = [(); 3].map(|()| placed(TYPE_KIND_NAMED, &[]));
        let of_the_program = placed(TYPE_KIND_UNION, &[b, a]);
        let list: &'static [usize] = vec![1, of_the_program].leak();
        let types: &'static mut [usize] = vec![0; PROGRAM_DESCRIPTIONS / WORD + 2].leak();
        types[PROGRAM_BUILT_TYPES / WORD] = list.as_ptr() as usize;
        let program = Program {
            stack_maps: crate::tests::NO_STACK_MAPS.as_ptr() as usize,
            types: types.as_ptr() as usize,
            unwind: 0,
            functions: 0,
        };
        // SAFETY: both tables are in the compiler's formats, and every
        // argument below is a live type object.
        unsafe {
            let mutator = Mutator::new(program, false);
            let union = |members: &[usize]| {
                baml_rt_type_intern(
                    &mutator,
                    TYPE_KIND_UNION as usize,
                    0,
                    members.as_ptr(),
                    members.len(),
                )
            };
            assert_eq!(union(&[a, b]), of_the_program);
            assert_eq!(union(&[b, a, b]), of_the_program);
            let made = union(&[c, a]);
            assert_ne!(made, of_the_program);
            assert_eq!(union(&[a, c]), made);
            assert_eq!(kind(made), TYPE_KIND_UNION);
            let mut members = [a, c];
            members.sort_unstable();
            assert_eq!(arguments(made), members);
            let all = union(&[made, b]);
            assert_eq!(union(&[b, c, a]), all);
            assert_eq!(union(&[of_the_program, made]), all);
            assert_eq!(arguments(all).len(), 3);
            assert_eq!(union(&[a, a]), a);
        }
    }
}
