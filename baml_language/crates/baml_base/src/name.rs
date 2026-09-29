//! Compact compiler names with BAML-owned serialization.

use std::{borrow::Borrow, convert::Infallible, fmt, io, ops::Deref, str::FromStr};

use borsh::{BorshDeserialize, BorshSerialize};
use smol_str::SmolStr;

/// An immutable string used for identifiers, keywords, and package names.
///
/// Keep the storage and comparison behavior of `SmolStr`, but own the Borsh
/// implementation so downstream builds need no `smol_str/borsh` feature.
#[repr(transparent)]
#[derive(
    Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct Name(SmolStr);

impl Name {
    #[inline]
    pub fn new(text: impl AsRef<str>) -> Self {
        Self(SmolStr::new(text))
    }

    /// Construct a name without allocating.
    ///
    /// # Panics
    /// Panics if `text` is longer than 23 bytes.
    pub const fn new_inline(text: &str) -> Self {
        Self(SmolStr::new_inline(text))
    }

    pub const fn new_static(text: &'static str) -> Self {
        Self(SmolStr::new_static(text))
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[inline]
    pub const fn is_heap_allocated(&self) -> bool {
        self.0.is_heap_allocated()
    }
}

// Preserve the old alias's debug output, including compiler snapshots.
impl fmt::Debug for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Deref for Name {
    type Target = str;

    #[inline]
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for Name {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for Name {
    #[inline]
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl From<&str> for Name {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

impl From<String> for Name {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

impl From<&String> for Name {
    fn from(text: &String) -> Self {
        Self::new(text)
    }
}

impl From<SmolStr> for Name {
    fn from(text: SmolStr) -> Self {
        Self(text)
    }
}

impl From<Name> for SmolStr {
    fn from(name: Name) -> Self {
        name.0
    }
}

impl From<Name> for String {
    fn from(name: Name) -> Self {
        name.0.into()
    }
}

impl FromStr for Name {
    type Err = Infallible;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(text))
    }
}

macro_rules! impl_string_eq {
    ($($ty:ty),* $(,)?) => {
        $(
            impl PartialEq<$ty> for Name {
                fn eq(&self, other: &$ty) -> bool {
                    self.as_str() == <$ty as AsRef<str>>::as_ref(other)
                }
            }

            impl PartialEq<Name> for $ty {
                fn eq(&self, other: &Name) -> bool {
                    other == self
                }
            }
        )*
    };
}

impl_string_eq!(str, &str, String, &String);

impl BorshSerialize for Name {
    fn serialize<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        // Identical to the former SmolStr implementation: a u32 byte length
        // followed by UTF-8. The wrapper adds no tag or framing.
        BorshSerialize::serialize(self.as_str(), writer)
    }
}

impl BorshDeserialize for Name {
    fn deserialize_reader<R: io::Read>(reader: &mut R) -> io::Result<Self> {
        let len = u32::deserialize_reader(reader)?;
        // Preserve allocation-free decoding of short identifiers. SmolStr 0.3
        // holds up to 23 bytes inline; use its safe constructor after validation.
        let mut inline = [0_u8; 23];
        if len as usize <= inline.len() {
            let bytes = &mut inline[..len as usize];
            reader.read_exact(bytes)?;
            let text = std::str::from_utf8(bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            return Ok(Self::new_inline(text));
        }

        // Reuse Borsh's chunked byte-vector reader rather than allocating the
        // entire length declared by a potentially truncated artifact upfront.
        let bytes = u8::vec_from_reader(len, reader)?
            .ok_or_else(|| io::Error::other("Borsh byte-vector reader is unavailable"))?;
        let text = String::from_utf8(bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        Ok(Self::from(text))
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, hash::BuildHasher};

    use super::*;

    #[test]
    fn preserves_storage_and_string_semantics() {
        assert_eq!(size_of::<Name>(), size_of::<SmolStr>());
        assert_eq!(align_of::<Name>(), align_of::<SmolStr>());
        assert_eq!(size_of::<Option<Name>>(), size_of::<Option<SmolStr>>());

        for text in ["", "short", "a name longer than the inline storage", "名前"] {
            let name = Name::new(text);
            let original = SmolStr::new(text);
            assert_eq!(format!("{name:?}"), format!("{original:?}"));
            assert_eq!(name.to_string(), text);
            assert_eq!(name.is_heap_allocated(), original.is_heap_allocated());
            let names = HashMap::from([(name.clone(), 42)]);
            assert_eq!(names.get(text), Some(&42));
            assert_eq!(
                names.hasher().hash_one(&name),
                names.hasher().hash_one(&original)
            );
        }

        let mut names = [Name::new("z"), Name::new("a"), Name::new("é")];
        names.sort();
        assert_eq!(names.map(String::from), ["a", "z", "é"]);
    }

    #[test]
    fn borsh_preserves_legacy_string_bytes() {
        for text in [
            "",
            "short",
            "12345678901234567890123",
            "123456789012345678901234",
            "名前",
            "é".repeat(128).as_str(),
        ] {
            let expected = borsh::to_vec(text).unwrap();
            assert_eq!(borsh::to_vec(&Name::new(text)).unwrap(), expected);
            let restored = borsh::from_slice::<Name>(&expected).unwrap();
            assert_eq!(restored, text);
            assert_eq!(
                restored.is_heap_allocated(),
                SmolStr::new(text).is_heap_allocated()
            );
        }
        assert_eq!(
            borsh::to_vec(&Name::new("é")).unwrap(),
            [2, 0, 0, 0, 0xc3, 0xa9]
        );

        // Names nested in artifact structures keep their old framing too.
        let legacy = vec![
            ("pkg".to_owned(), Some("名前".to_owned())),
            ("item".to_owned(), None),
        ];
        let names: Vec<(Name, Option<Name>)> =
            borsh::from_slice(&borsh::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(
            borsh::to_vec(&names).unwrap(),
            borsh::to_vec(&legacy).unwrap()
        );
    }

    #[test]
    fn borsh_rejects_malformed_names() {
        for bytes in [
            &[][..],
            &[1, 0, 0],
            &[2, 0, 0, 0, b'a'],
            &[1, 0, 0, 0, 0xff],
            &[24, 0, 0, 0, b'a'],
            &[0xff, 0xff, 0xff, 0xff],
        ] {
            assert!(borsh::from_slice::<Name>(bytes).is_err(), "{bytes:?}");
        }
        let mut invalid_utf8 = vec![24, 0, 0, 0];
        invalid_utf8.extend([0xff; 24]);
        assert!(borsh::from_slice::<Name>(&invalid_utf8).is_err());
    }

    #[test]
    fn serde_preserves_string_representation() {
        let name = Name::new("quoted \"名前\"");
        let json = serde_json::to_string(&name).unwrap();
        assert_eq!(json, serde_json::to_string(name.as_str()).unwrap());
        assert_eq!(serde_json::from_str::<Name>(&json).unwrap(), name);
    }
}
