use itertools::Itertools;

use crate::{
    ir_type::{TypeGeneric, UnionTypeGeneric},
    type_meta, ConstraintLevel,
};

/// Strip metadata from a type for comparison purposes.
fn without_meta(t: &TypeGeneric<type_meta::IR>) -> TypeGeneric<type_meta::IR> {
    let mut cloned = t.clone();
    cloned.set_meta(type_meta::IR::default());
    cloned
}

/// Check if `candidate` is a subtype of `target`.
/// A type X is a subtype of Y if:
/// - Y is a union and X is one of Y's inner types (ignoring metadata)
fn is_subtype_of(
    candidate: &TypeGeneric<type_meta::IR>,
    target: &TypeGeneric<type_meta::IR>,
) -> bool {
    // If target is a union, check if candidate matches any inner type (ignoring metadata)
    if let TypeGeneric::Union(inner, _) = target {
        let candidate_without_meta = without_meta(candidate);
        for inner_type in inner.types.iter() {
            let inner_without_meta = without_meta(inner_type);
            if candidate_without_meta == inner_without_meta {
                return true;
            }
        }
    }
    false
}

/// Check if any variant contains null internally (for optional union absorption)
fn any_variant_contains_null(variants: &[TypeGeneric<type_meta::IR>]) -> bool {
    for variant in variants {
        if let TypeGeneric::Union(inner, _) = variant {
            if inner.is_optional() {
                return true;
            }
        }
    }
    false
}

/// Remove variants that are subtypes of other variants.
/// When X <: Y, X gets absorbed into Y (remove X, keep Y).
fn absorb_subtypes(variants: Vec<TypeGeneric<type_meta::IR>>) -> Vec<TypeGeneric<type_meta::IR>> {
    let mut to_remove = vec![];

    for (i, candidate) in variants.iter().enumerate() {
        for (j, target) in variants.iter().enumerate() {
            if i == j {
                continue;
            }
            if is_subtype_of(candidate, target) {
                to_remove.push(i);
                break;
            }
        }
    }

    variants
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !to_remove.contains(i))
        .map(|(_, v)| v)
        .collect()
}

impl TypeGeneric<type_meta::IR> {
    pub fn simplify(&self) -> Self {
        match self {
            TypeGeneric::Union(inner, union_meta) => {
                let view = inner.view();
                let flattened = view.flatten();
                let unique = flattened.into_iter().unique().collect::<Vec<_>>();
                let has_null = unique.contains(&TypeGeneric::null());
                // if the union contains null, we'll detect that here.
                let mut variants: Vec<TypeGeneric<type_meta::IR>> = unique
                    .into_iter()
                    .filter(|t| t != &TypeGeneric::null())
                    .collect::<Vec<_>>();

                // Absorb variants that are subtypes of other variants
                // e.g., (A | B) @check | B => (A | B) @check (B absorbed)
                variants = absorb_subtypes(variants);

                // Check if null is absorbed by a variant that contains it internally
                // e.g., (int | null) @check | null => (int | null) @check
                let null_absorbed = has_null && any_variant_contains_null(&variants);
                let has_null = has_null && !null_absorbed;

                // here metadata simplification of both variants and the union itself happens
                // unions will never have checks and asserts in their own metadata, always distributed and do not keep
                // Union(A|B)(@check(A, {..})) => Union(A@check(A, {..})|B@check(B, {..}))
                let (to_move, to_keep): (Vec<_>, Vec<_>) =
                    union_meta.constraints.clone().into_iter().partition(|c| {
                        // move these
                        matches!(c.level, ConstraintLevel::Check | ConstraintLevel::Assert)
                    });

                let type_meta::base::StreamingBehavior {
                    done,
                    needed,
                    state,
                } = union_meta.streaming_behavior;

                // Add to_move to each variant
                for variant in variants.iter_mut() {
                    variant.meta_mut().constraints.extend(to_move.clone());
                    if done {
                        variant.meta_mut().streaming_behavior.done = true;
                    }
                    if needed {
                        variant.meta_mut().streaming_behavior.needed = true;
                    }
                }

                let mut new_meta = type_meta::IR::default();
                new_meta.constraints.extend(to_keep);

                if needed {
                    new_meta.streaming_behavior.needed = true;
                }
                new_meta.streaming_behavior.state = state;
                new_meta.streaming_behavior.done = done;

                let simplified: TypeGeneric<type_meta::IR> = match variants.len() {
                    0 => return TypeGeneric::null(),
                    1 => {
                        if has_null {
                            // Return an optional of a single variant.
                            TypeGeneric::Union(
                                unsafe { UnionTypeGeneric::new_unsafe(vec![variants[0].clone()]) },
                                new_meta,
                            )
                        } else {
                            // Return the single variant.
                            variants[0].clone()
                        }
                    }
                    _ => {
                        if has_null {
                            variants.push(TypeGeneric::null());
                        }
                        TypeGeneric::Union(
                            unsafe { UnionTypeGeneric::new_unsafe(variants) },
                            new_meta,
                        )
                    }
                };

                simplified
            }
            _ => self.clone(),
        }
    }
}
