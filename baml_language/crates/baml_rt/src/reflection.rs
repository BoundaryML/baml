//! Closed-world type values. The compiler freezes identity and display tables;
//! managed values only carry an index into those immutable tables.
use crate::{
    Dyn, R,
    strings::{Cx, Str},
    telemetry::capture::Type,
};
use baml_rt_gc::{Descriptor, Kind};
pub use baml_type::type_kind::TypeKind;

pub struct StaticType {
    pub ty: &'static Type,
    pub display: &'static str,
    /// Equivalence is computed with the compiler's complete type facts. Keep
    /// the spelled type separately: union order remains visible when rendered.
    pub identity: usize,
    pub kind: TypeKind,
    pub children: &'static [usize],
}

pub const fn descriptor(type_id: u32) -> Descriptor {
    Descriptor {
        kind: Kind::Opaque,
        size: 16,
        ptr_slots: (0, 0),
        tagged_slots: (0, 0),
        elem: None,
        type_id,
        fields: &[],
        finalizer: None,
    }
}

pub fn new(desc: u32, index: usize) -> R<usize> {
    let object = unsafe { crate::try_alloc(desc, 16)? };
    unsafe {
        object.add(8).cast::<usize>().write(index);
    }
    Ok(object as usize)
}

/// # Safety
/// `value` is a live reflected type allocation from this program.
pub unsafe fn index(value: usize) -> usize {
    unsafe { *((value + 8) as *const usize) }
}

/// # Safety
/// Non-null values bearing `tag` have this program's reflected type layout.
pub unsafe fn equal(types: &[StaticType], tag: u64, left: Dyn, right: Dyn) -> bool {
    if left.tag == 2 || right.tag == 2 {
        return left.tag == right.tag;
    }
    if left.tag != tag || right.tag != tag || left.payload == 0 || right.payload == 0 {
        return false;
    }
    unsafe {
        types[index(left.payload as usize)].identity
            == types[index(right.payload as usize)].identity
    }
}

/// # Safety
/// `value` is a live type allocation indexed into `types`.
pub unsafe fn text(types: &[StaticType], value: usize) -> R<Str> {
    let value = crate::nonnull(value)?;
    unsafe { Cx::current().string(types[index(value)].display) }
}

/// # Safety
/// `value` is a live reflected type allocation indexed into `types`.
pub unsafe fn kind(types: &[StaticType], value: usize) -> TypeKind {
    unsafe { types[index(value)].kind }
}

/// # Safety
/// `value` is a type allocation of a kind exposing the requested child.
pub unsafe fn child(types: &[StaticType], desc: u32, value: usize, child: usize) -> R<usize> {
    unsafe { new(desc, types[index(value)].children[child]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TYPES: &[StaticType] = &[
        StaticType {
            ty: &Type::Int,
            display: "int",
            identity: 0,
            kind: TypeKind::Primitive,
            children: &[],
        },
        StaticType {
            ty: &Type::Bool,
            display: "bool",
            identity: 1,
            kind: TypeKind::Primitive,
            children: &[],
        },
        StaticType {
            ty: &Type::Union(&[Type::String, Type::Int]),
            display: "string | int",
            identity: 2,
            kind: TypeKind::Union,
            children: &[],
        },
        StaticType {
            ty: &Type::Union(&[Type::Int, Type::String]),
            display: "int | string",
            identity: 2,
            kind: TypeKind::Union,
            children: &[],
        },
    ];
    static DESCRIPTORS: &[Descriptor] = &[crate::STRING_DESC, descriptor(4)];

    #[test]
    fn identity_is_separate_from_spelling_and_survives_collection() {
        let mut instance = crate::Instance::new(DESCRIPTORS, &[false, false, false, true, true]);
        instance.set_stress(true);
        unsafe {
            instance.run_owned(|| -> R<()> {
                let first = new(1, 2)?;
                let second = new(1, 3)?;
                let boolean = new(1, 1)?;
                crate::tasks::collect()?;
                let tagged = |value| Dyn {
                    tag: 4,
                    payload: value as u64,
                };
                assert!(equal(TYPES, 4, tagged(first), tagged(second)));
                assert!(!equal(TYPES, 4, tagged(first), tagged(boolean)));
                assert!(!equal(TYPES, 4, tagged(first), Dyn { tag: 0, payload: 2 }));
                assert!(equal(
                    TYPES,
                    4,
                    Dyn { tag: 2, payload: 0 },
                    Dyn { tag: 2, payload: 0 }
                ));
                assert_eq!(text(TYPES, first)?.to_owned(), "string | int");
                assert_eq!(text(TYPES, second)?.to_owned(), "int | string");
                Ok(())
            })
        }
        .unwrap();
    }
}
