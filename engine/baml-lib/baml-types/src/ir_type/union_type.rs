use super::{type_meta, TypeGeneric, UnionTypeGeneric};

impl<T: std::fmt::Debug + Default> UnionTypeGeneric<T> {
    // disallow construction so people have to use:
    // FieldType::union(vec![...]) which does a simplify() default
    fn new(types: Vec<TypeGeneric<T>>) -> Self {
        Self {
            types,
            null_type: Box::new(TypeGeneric::null()),
        }
    }

    pub(crate) unsafe fn new_unsafe(types: Vec<TypeGeneric<T>>) -> Self {
        if types.iter().all(|t| t.is_null()) {
            panic!("FATAL, please report this bug: Union type must have at least one non-null type. Got {types:?}");
        }
        Self {
            types,
            null_type: Box::new(TypeGeneric::null()),
        }
    }
}

pub trait UnionConstructor<Meta> {
    fn union(choices: Vec<TypeGeneric<Meta>>) -> TypeGeneric<Meta>;
}

impl UnionConstructor<type_meta::IR> for TypeGeneric<type_meta::IR> {
    fn union(mut choices: Vec<TypeGeneric<type_meta::IR>>) -> TypeGeneric<type_meta::IR> {
        if choices.len() == 1 {
            return choices.remove(0);
        }
        TypeGeneric::Union(UnionTypeGeneric::new(choices), type_meta::IR::default()).simplify()
    }
}

impl UnionConstructor<type_meta::NonStreaming> for TypeGeneric<type_meta::NonStreaming> {
    fn union(
        mut choices: Vec<TypeGeneric<type_meta::NonStreaming>>,
    ) -> TypeGeneric<type_meta::NonStreaming> {
        if choices.len() == 1 {
            return choices.remove(0);
        }
        TypeGeneric::Union(
            UnionTypeGeneric::new(choices),
            type_meta::NonStreaming::default(),
        )
        .simplify()
    }
}

impl TypeGeneric<type_meta::IR> {
    pub fn union_with_meta(
        choices: Vec<TypeGeneric<type_meta::IR>>,
        meta: type_meta::IR,
    ) -> TypeGeneric<type_meta::IR> {
        TypeGeneric::Union(unsafe { UnionTypeGeneric::new_unsafe(choices) }, meta).simplify()
    }

    pub fn optional(inner: TypeGeneric<type_meta::IR>) -> TypeGeneric<type_meta::IR> {
        if inner.is_null() {
            return inner;
        }
        TypeGeneric::<type_meta::IR>::union(vec![inner, TypeGeneric::null()])
    }

    pub fn as_optional(self) -> TypeGeneric<type_meta::IR> {
        TypeGeneric::optional(self)
    }
}
