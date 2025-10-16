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
pub trait EncodeToBuffer<As, Lookup>
where
    As: prost::Message,
{
    fn encode_to_c_buffer(&self, lookup: &Lookup, mode: baml_types::StreamingMode) -> Vec<u8>;
    // Higher-ranked bound: *for any* borrow `'a` of `lookup`,
    // the IR wrapper knows how to `Encode` into our prost type.
}

/// Blanket implementation: any `T` that fulfils the bounds below
/// automatically gains `encode_to_c_buffer`.
impl<Item, Lookup, As> EncodeToBuffer<As, Lookup> for Item
where
    As: prost::Message,
    Lookup: baml_types::baml_value::TypeLookups,
    for<'a> WithIr<'a, Item, Lookup>: Encode<As>,
{
    fn encode_to_c_buffer(&self, lookup: &Lookup, mode: baml_types::StreamingMode) -> Vec<u8> {
        // 1. Build the IR & convert to the prost message --------------------
        let msg: As = WithIr {
            value: self,
            lookup,
            mode,
        }
        .encode();

        // 2. Prost-encode into a Vec<u8> ------------------------------------
        msg.encode_to_vec()
    }
}

pub(super) struct WithIr<'a, T, TypeLookups: baml_types::baml_value::TypeLookups> {
    pub value: &'a T,
    pub lookup: &'a TypeLookups,
    pub mode: baml_types::StreamingMode,
}
