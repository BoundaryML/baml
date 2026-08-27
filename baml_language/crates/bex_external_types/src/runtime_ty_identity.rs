use baml_type::{Name, RuntimeTy};

/// Exact semantic identity for runtime types, ignoring source-only attributes
/// and union member ordering.
pub fn runtime_ty_structurally_equal(left: &RuntimeTy, right: &RuntimeTy) -> bool {
    use RuntimeTy as T;
    match (left, right) {
        (T::String { .. }, T::String { .. })
        | (T::Int { .. }, T::Int { .. })
        | (T::Bigint { .. }, T::Bigint { .. })
        | (T::Float { .. }, T::Float { .. })
        | (T::Bool { .. }, T::Bool { .. })
        | (T::Null { .. }, T::Null { .. })
        | (T::Uint8Array { .. }, T::Uint8Array { .. }) => true,
        (T::Unknown { .. }, T::Unknown { .. })
        | (T::RustType { .. }, T::RustType { .. })
        | (T::Type { .. }, T::Type { .. })
        | (T::Resource { .. }, T::Resource { .. })
        | (T::PromptAst { .. }, T::PromptAst { .. })
        | (T::Void { .. }, T::Void { .. })
        | (T::Never { .. }, T::Never { .. }) => true,
        (T::Media(left, _), T::Media(right, _)) => left == right,
        (T::Literal(left, ..), T::Literal(right, ..)) => left == right,
        (T::List(left, _), T::List(right, _)) => runtime_ty_structurally_equal(left, right),
        (
            T::Map {
                key: left_key,
                value: left_value,
                ..
            },
            T::Map {
                key: right_key,
                value: right_value,
                ..
            },
        ) => {
            runtime_ty_structurally_equal(left_key, right_key)
                && runtime_ty_structurally_equal(left_value, right_value)
        }
        (T::Class(left_name, left_args, _), T::Class(right_name, right_args, _)) => {
            left_name == right_name && structurally_equal_slices(left_args, right_args)
        }
        (
            T::Interface(left_name, left_args, left_bindings, _),
            T::Interface(right_name, right_args, right_bindings, _),
        ) => {
            left_name == right_name
                && structurally_equal_slices(left_args, right_args)
                && structurally_equal_named_types(left_bindings, right_bindings)
        }
        (T::Enum(left, _), T::Enum(right, _)) => left == right,
        (
            T::EnumVariant(left_name, left_variant, _),
            T::EnumVariant(right_name, right_variant, _),
        ) => left_name == right_name && left_variant == right_variant,
        (T::TypeAlias(left, _), T::TypeAlias(right, _)) => left == right,
        (
            T::Function {
                params: left_params,
                ret: left_ret,
                throws: left_throws,
                ..
            },
            T::Function {
                params: right_params,
                ret: right_ret,
                throws: right_throws,
                ..
            },
        ) => {
            left_params.len() == right_params.len()
                && left_params.iter().zip(right_params).all(|(left, right)| {
                    left.name == right.name
                        && left.mode == right.mode
                        && runtime_ty_structurally_equal(&left.ty, &right.ty)
                })
                && runtime_ty_structurally_equal(left_ret, right_ret)
                && runtime_ty_structurally_equal(left_throws, right_throws)
        }
        (T::Union(left, _), T::Union(right, _)) => structurally_equal_unordered_slices(left, right),
        _ => false,
    }
}

fn structurally_equal_slices(left: &[RuntimeTy], right: &[RuntimeTy]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| runtime_ty_structurally_equal(left, right))
}

fn structurally_equal_unordered_slices(left: &[RuntimeTy], right: &[RuntimeTy]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let mut matched_right = vec![false; right.len()];
    left.iter().all(|left_member| {
        let Some(index) = right.iter().enumerate().position(|(index, right_member)| {
            !matched_right[index] && runtime_ty_structurally_equal(left_member, right_member)
        }) else {
            return false;
        };
        matched_right[index] = true;
        true
    })
}

fn structurally_equal_named_types(left: &[(Name, RuntimeTy)], right: &[(Name, RuntimeTy)]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let mut matched_right = vec![false; right.len()];
    left.iter().all(|(left_name, left_ty)| {
        let Some(index) = right
            .iter()
            .enumerate()
            .position(|(index, (right_name, right_ty))| {
                !matched_right[index]
                    && left_name == right_name
                    && runtime_ty_structurally_equal(left_ty, right_ty)
            })
        else {
            return false;
        };
        matched_right[index] = true;
        true
    })
}

/// Compare a selected union arm while tolerating the legacy root-level
/// representation where a non-null arm may be wrapped in `T | null`.
pub fn selected_arm_equal(left: &RuntimeTy, right: &RuntimeTy) -> bool {
    if runtime_ty_structurally_equal(left, right) {
        return true;
    }
    sole_non_null(left).is_some_and(|inner| runtime_ty_structurally_equal(inner, right))
        || sole_non_null(right).is_some_and(|inner| runtime_ty_structurally_equal(left, inner))
}

fn sole_non_null(ty: &RuntimeTy) -> Option<&RuntimeTy> {
    let RuntimeTy::Union(members, _) = ty else {
        return None;
    };
    if !members.iter().any(RuntimeTy::is_null) {
        return None;
    }
    let mut non_null = members.iter().filter(|member| !member.is_null());
    let only = non_null.next()?;
    non_null.next().is_none().then_some(only)
}

#[cfg(test)]
mod tests {
    use baml_type::{RuntimeTy, TyAttr};

    use super::runtime_ty_structurally_equal;

    fn union(members: Vec<RuntimeTy>) -> RuntimeTy {
        RuntimeTy::Union(members, TyAttr::default())
    }

    #[test]
    fn union_structural_equality_ignores_member_order() {
        let left = union(vec![RuntimeTy::string(), RuntimeTy::int()]);
        let right = union(vec![RuntimeTy::int(), RuntimeTy::string()]);

        assert!(runtime_ty_structurally_equal(&left, &right));
        assert!(runtime_ty_structurally_equal(&right, &left));
    }

    #[test]
    fn union_structural_equality_preserves_duplicate_multiplicity() {
        let left = union(vec![
            RuntimeTy::string(),
            RuntimeTy::string(),
            RuntimeTy::int(),
        ]);
        let right = union(vec![
            RuntimeTy::string(),
            RuntimeTy::int(),
            RuntimeTy::int(),
        ]);

        assert!(!runtime_ty_structurally_equal(&left, &right));
        assert!(!runtime_ty_structurally_equal(&right, &left));
    }
}
