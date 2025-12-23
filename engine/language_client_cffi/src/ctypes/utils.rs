use baml_types::{ir_type::TypeGeneric, HasType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnionAllowance {
    Allow,
    Disallow,
}

pub trait IsChecked {
    fn checks(&self) -> Option<Vec<&str>>;
    fn stream_with_state(&self) -> bool;
    fn pop_stream_state(&mut self);
    fn pop_checks(&mut self);
}

impl IsChecked for baml_types::type_meta::base::TypeMeta {
    fn checks(&self) -> Option<Vec<&str>> {
        let checks: Vec<_> = self
            .constraints
            .iter()
            .filter_map(|c| {
                if c.level == baml_types::ConstraintLevel::Check {
                    c.label.as_deref()
                } else {
                    None
                }
            })
            .collect();

        if checks.is_empty() {
            None
        } else {
            Some(checks)
        }
    }

    fn stream_with_state(&self) -> bool {
        self.streaming_behavior.state
    }

    fn pop_stream_state(&mut self) {
        self.streaming_behavior.state = false;
    }

    fn pop_checks(&mut self) {
        self.constraints.clear();
    }
}

impl IsChecked for baml_types::type_meta::NonStreaming {
    fn checks(&self) -> Option<Vec<&str>> {
        let checks: Vec<_> = self
            .constraints
            .iter()
            .filter_map(|c| {
                if c.level == baml_types::ConstraintLevel::Check {
                    c.label.as_deref()
                } else {
                    None
                }
            })
            .collect();

        if checks.is_empty() {
            None
        } else {
            Some(checks)
        }
    }
    fn stream_with_state(&self) -> bool {
        false
    }
    fn pop_stream_state(&mut self) {
        // No-op
    }
    fn pop_checks(&mut self) {
        self.constraints.clear();
    }
}

impl IsChecked for baml_types::type_meta::Streaming {
    fn checks(&self) -> Option<Vec<&str>> {
        let checks: Vec<_> = self
            .constraints
            .iter()
            .filter_map(|c| {
                if c.level == baml_types::ConstraintLevel::Check {
                    c.label.as_deref()
                } else {
                    None
                }
            })
            .collect();

        if checks.is_empty() {
            None
        } else {
            Some(checks)
        }
    }
    fn stream_with_state(&self) -> bool {
        self.streaming_behavior.state
    }
    fn pop_stream_state(&mut self) {
        self.streaming_behavior.state = false;
    }
    fn pop_checks(&mut self) {
        self.constraints.clear();
    }
}

pub trait Encode<To> {
    fn encode(self) -> To;
}

pub(super) trait Decode {
    type From;

    fn decode(from: Self::From) -> Result<Self, anyhow::Error>
    where
        Self: Sized;
}

pub trait DecodeFromBuffer {
    fn from_c_buffer(buffer: *const libc::c_char, length: usize) -> Result<Self, anyhow::Error>
    where
        Self: Sized;
}

impl<T> DecodeFromBuffer for T
where
    T: Decode,
    T::From: prost::Message + Default,
{
    fn from_c_buffer(buffer: *const libc::c_char, length: usize) -> Result<Self, anyhow::Error>
    where
        Self: Sized,
    {
        use prost::Message;

        let buffer = unsafe { std::slice::from_raw_parts(buffer as *const u8, length) };
        let root = T::From::decode(buffer)?;

        Self::decode(root)
    }
}

/// automatically gains `encode_to_c_buffer`.
pub trait EncodeToBuffer<As, Lookup, TyMeta>
where
    As: prost::Message,
{
    fn encode_to_c_buffer(&self, lookup: &Lookup, mode: baml_types::StreamingMode) -> Vec<u8>;
    // Higher-ranked bound: *for any* borrow `'a` of `lookup`,
    // the IR wrapper knows how to `Encode` into our prost type.
}

/// Blanket implementation: any `T` that fulfils the bounds below
/// automatically gains `encode_to_c_buffer`.
impl<Item, Lookup, As, TyMeta> EncodeToBuffer<As, Lookup, TyMeta> for Item
where
    As: prost::Message,
    Lookup: baml_types::baml_value::TypeLookups,
    Item: HasType<TyMeta>,
    TyMeta: Clone,
    for<'a> WithIr<'a, Item, Lookup, TyMeta>: Encode<As>,
{
    fn encode_to_c_buffer(&self, lookup: &Lookup, mode: baml_types::StreamingMode) -> Vec<u8> {
        // 1. Build the IR & convert to the prost message --------------------
        let msg: As = WithIr {
            value: self,
            lookup,
            mode,
            curr_type: self.field_type().clone(),
        }
        .encode();

        // 2. Prost-encode into a Vec<u8> ------------------------------------
        msg.encode_to_vec()
    }
}

pub(super) struct WithIr<'a, T, TypeLookups: baml_types::baml_value::TypeLookups, TyMeta> {
    pub value: &'a T,
    pub lookup: &'a TypeLookups,
    pub mode: baml_types::StreamingMode,
    pub curr_type: TypeGeneric<TyMeta>,
}
