use crate::{
    baml_value::TypeLookups,
    ir_type::{ArrowGeneric, TypeStreaming, UnionTypeGeneric},
    type_meta, StreamingMode, TypeIR, TypeValue,
};

pub fn from_type_ir(r#type: &TypeIR, lookup: &impl TypeLookups) -> TypeStreaming {
    // This inner worker function goes from `FieldType` to `FieldType` to be
    // suitable for recursive use. We only wrap the outermost `FieldType` in
    // `StreamingType`.
    fn partialize_helper(r#type: &TypeIR, lookup: &impl TypeLookups) -> TypeStreaming {
        let type_meta::base::StreamingBehavior {
            done,
            mut needed,
            state,
        } = r#type
            .streaming_behavior()
            .combine(&inherent_streaming_behavior(r#type, lookup));

        // A copy of the metadata to use in the new type.
        let meta = type_meta::Streaming {
            streaming_behavior: type_meta::stream::StreamingBehavior { done, state },
            constraints: r#type.meta().constraints.clone(),
        };

        // Streaming behavior of the type, without regard to the `@stream` annotations.
        // (That annotation will be handled later in this function).
        let mut base_type_streaming = match r#type {
            TypeIR::Top(_) => TypeStreaming::Top(meta),
            TypeIR::Primitive(type_value, _) => match type_value {
                TypeValue::Null => TypeStreaming::Primitive(TypeValue::Null, meta),
                TypeValue::Int => TypeStreaming::Primitive(TypeValue::Int, meta),
                TypeValue::Float => TypeStreaming::Primitive(TypeValue::Float, meta),
                TypeValue::Bool => TypeStreaming::Primitive(TypeValue::Bool, meta),
                TypeValue::String => TypeStreaming::Primitive(TypeValue::String, meta),
                TypeValue::Media(media_type) => {
                    TypeStreaming::Primitive(TypeValue::Media(*media_type), meta)
                }
            },
            TypeIR::Enum { name, dynamic, .. } => TypeStreaming::Enum {
                name: name.clone(),
                dynamic: *dynamic,
                meta: meta.clone(),
            },
            TypeIR::Literal(literal_value, _) => {
                TypeStreaming::Literal(literal_value.clone(), meta)
            }
            TypeIR::Class { name, dynamic, .. } => TypeStreaming::Class {
                name: name.clone(),
                mode: if done {
                    StreamingMode::NonStreaming
                } else {
                    StreamingMode::Streaming
                },
                dynamic: *dynamic,
                meta: meta.clone(),
            },
            TypeIR::List(item_type, _) => {
                needed = true;
                // items inside of arrays don't need to be nullable.
                // If @stream.done is on the list, propagate it to inner elements.
                let mut item_type = item_type.clone();
                item_type.meta_mut().streaming_behavior.needed = true;
                if done {
                    item_type.meta_mut().streaming_behavior.done = true;
                }
                TypeStreaming::List(Box::new(from_type_ir(&item_type, lookup)), meta)
            }
            TypeIR::Map(key_type, item_type, _) => {
                needed = true;
                TypeStreaming::Map(
                    {
                        // Keys cannot be null in maps
                        let mut clone = key_type.clone();
                        clone.meta_mut().streaming_behavior.needed = true;
                        if done {
                            clone.meta_mut().streaming_behavior.done = true;
                        }
                        Box::new(from_type_ir(&clone, lookup))
                    },
                    {
                        // values don't need to be nullable.
                        // If @stream.done is on the map, propagate it to inner elements.
                        let mut item_type = item_type.clone();
                        item_type.meta_mut().streaming_behavior.needed = true;
                        if done {
                            item_type.meta_mut().streaming_behavior.done = true;
                        }
                        Box::new(from_type_ir(&item_type, lookup))
                    },
                    meta,
                )
            }
            TypeIR::RecursiveTypeAlias { name, .. } => TypeStreaming::RecursiveTypeAlias {
                name: name.clone(),
                mode: if done {
                    StreamingMode::NonStreaming
                } else {
                    StreamingMode::Streaming
                },
                meta: meta.clone(),
            },
            TypeIR::Tuple(field_types, _) => TypeStreaming::Tuple(
                field_types
                    .iter()
                    .map(|t| from_type_ir(t, lookup))
                    .collect(),
                meta,
            ),
            TypeIR::Arrow(arrow, _) => TypeStreaming::Arrow(
                Box::new(ArrowGeneric {
                    param_types: arrow
                        .param_types
                        .iter()
                        .map(|t| from_type_ir(t, lookup))
                        .collect(),
                    return_type: from_type_ir(&arrow.return_type, lookup),
                }),
                meta,
            ),
            TypeIR::Union(union_type, _) => {
                let is_optional = union_type.is_optional();
                let variants = union_type.iter_skip_null();
                let variants = variants.into_iter().cloned().map(|mut t| {
                    t.meta_mut().streaming_behavior.needed = true;
                    from_type_ir(&t, lookup)
                });

                let meta_needs_wrapping =
                    !meta.constraints.is_empty() || meta.streaming_behavior.done;

                let variants = if is_optional || (!meta_needs_wrapping && !needed) {
                    variants
                        .chain(std::iter::once(TypeStreaming::null()))
                        .collect()
                } else {
                    variants.collect()
                };
                TypeStreaming::Union(unsafe { UnionTypeGeneric::new_unsafe(variants) }, meta)
            }
        };
        if needed || base_type_streaming.is_optional() {
            // Needed streaming types, and streaming types that are optional, need
            // no further processing to add optionality.
            base_type_streaming
        } else {
            // Currently base_type_streaming has the interesting metadata.
            // In the union we create to make base_type_streaming optional,
            // we want that inner metadata to be default, our outer union to
            // have the metadata. So we create a new default metadata and swap
            // its memory with that of the inner base_type.

            let use_with_state = base_type_streaming.meta().streaming_behavior.state;
            base_type_streaming.meta_mut().streaming_behavior.state = false;

            let mut union = TypeStreaming::Union(
                unsafe {
                    UnionTypeGeneric::new_unsafe(vec![base_type_streaming, TypeStreaming::null()])
                },
                Default::default(),
            );
            if use_with_state {
                let meta = union.meta_mut();
                meta.streaming_behavior.state = use_with_state;
            }
            union
        }
    }

    // Types have inherent streaming behavior. For example literals and
    // numbers are inherently @done. These behaviors are applied even
    // without user annotations.
    fn inherent_streaming_behavior(
        field_type: &TypeIR,
        lookup: &impl TypeLookups,
    ) -> type_meta::base::StreamingBehavior {
        type StreamingBehavior = type_meta::base::StreamingBehavior;
        match field_type {
            TypeIR::Top(_) => Default::default(),
            TypeIR::Primitive(type_value, _) => match type_value {
                TypeValue::Bool | TypeValue::Float | TypeValue::Int => StreamingBehavior {
                    done: true,
                    ..Default::default()
                },
                TypeValue::String | TypeValue::Null | TypeValue::Media(_) => Default::default(),
            },
            TypeIR::Enum { .. } | TypeIR::Literal(_, _) => StreamingBehavior {
                done: true,
                ..Default::default()
            },
            TypeIR::RecursiveTypeAlias { name, .. } => match lookup.expand_recursive_type(name) {
                Ok(expansion) if expansion.is_optional() => StreamingBehavior {
                    needed: true,
                    ..Default::default()
                },
                _ => Default::default(),
            },
            TypeIR::Class { .. }
            | TypeIR::List(..)
            | TypeIR::Map(..)
            | TypeIR::Tuple(..)
            | TypeIR::Arrow(..)
            | TypeIR::Union(..) => Default::default(),
        }
    }
    partialize_helper(r#type, lookup)
}

pub fn to_type_ir(r#type: &TypeStreaming) -> TypeIR {
    r#type.map_meta(
        |type_meta::stream::TypeMetaStreaming {
             streaming_behavior,
             constraints,
         }| {
            type_meta::IR {
                streaming_behavior: type_meta::base::StreamingBehavior {
                    done: streaming_behavior.done,
                    state: streaming_behavior.state,
                    // stream types already include nulls, so we don't need to add them again
                    needed: true,
                },
                constraints: constraints.clone(),
            }
        },
    )
}
