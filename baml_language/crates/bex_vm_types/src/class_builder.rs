//! A `reflect.class.Builder`'s state: the class it is defining, field by
//! field, until `build` freezes it.

use crate::RetainedFootprint;

/// A runtime class being defined. Plain data: a field's type is a root the
/// builder's own `_roots` holds, named here by its index.
pub struct BuilderState {
    /// The builder's identity within its recursive group.
    pub id: u64,
    /// The name the class will have.
    pub name: String,
    /// The fields so far, in order.
    pub fields: Vec<BuilderField>,
    /// Whether `build` has frozen the builder.
    pub frozen: bool,
}

/// A field of a class being defined.
#[derive(Clone)]
pub struct BuilderField {
    pub name: String,
    /// The position of the field's type in the builder's `_roots`.
    pub root_index: usize,
    /// `@skip`. A `WithMeta` row has no slot for it, so only `fields_from`,
    /// which copies another class's fields, sets it.
    pub skip: bool,
}

impl RetainedFootprint for BuilderState {
    fn retained_bytes(&self) -> usize {
        self.name.capacity()
            + self.fields.capacity() * size_of::<BuilderField>()
            + self
                .fields
                .iter()
                .map(|field| field.name.capacity())
                .sum::<usize>()
    }
}
