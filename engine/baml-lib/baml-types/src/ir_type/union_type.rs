use super::{TypeGeneric, UnionTypeGeneric, type_meta};

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
            panic!("FATAL, please report this bug: Union type must have at least one non-null type. Got {:?}", types);
        }
        Self {
            types,
            null_type: Box::new(TypeGeneric::null()),
        }
    }
}

impl TypeGeneric<type_meta::Base> {
    pub fn union(choices: Vec<TypeGeneric<type_meta::Base>>) -> TypeGeneric<type_meta::Base> {
        TypeGeneric::Union(UnionTypeGeneric::new(choices), type_meta::Base::default()).simplify()
    }

    pub fn union_with_meta(choices: Vec<TypeGeneric<type_meta::Base>>, meta: type_meta::Base) -> TypeGeneric<type_meta::Base> {
        TypeGeneric::Union(unsafe { UnionTypeGeneric::new_unsafe(choices) }, meta).simplify()
    }

    pub fn optional(inner: TypeGeneric<type_meta::Base>) -> TypeGeneric<type_meta::Base> {
        if inner.is_null() {
            return inner;
        }
        TypeGeneric::union(vec![inner, TypeGeneric::null()])
    }

    pub fn as_optional(self) -> TypeGeneric<type_meta::Base> {
        TypeGeneric::optional(self)
    }
}
