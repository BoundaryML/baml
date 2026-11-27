//! Values inside a session: reading them, and copying Rust values in and
//! out of BAML memory.

use crate::{Fns, Library, Map, RawValue, SdkError, Session, tag, text_from};

/// A value inside a [`Session`]: a scalar, or an object it holds a handle
/// to, which is freed when the value is dropped. It cannot outlive the
/// session, and like it, cannot cross to another thread: [`Detached`] can.
pub struct BamlValue<'s> {
    raw: RawValue,
    session: &'s Session,
}

impl Drop for BamlValue<'_> {
    fn drop(&mut self) {
        if self.raw.tag & tag::HANDLE != 0 {
            // SAFETY: a handle this value owns, under the session's lock.
            unsafe { (self.session.fns().handle_free)(self.raw.payload) };
        }
    }
}

/// A value carried past its session: it may outlive the session and cross
/// threads. [`Session::attach`] brings it back. Dropped without that, its
/// handle is freed at the next session.
pub struct Detached {
    raw: RawValue,
    library: &'static Library,
}

impl Detached {
    pub(crate) fn into_raw(self) -> RawValue {
        let raw = self.raw;
        std::mem::forget(self);
        raw
    }
}

impl Drop for Detached {
    fn drop(&mut self) {
        if self.raw.tag & tag::HANDLE != 0 {
            self.library.free_later(self.raw.payload);
        }
    }
}

fn decode(what: impl Into<String>) -> SdkError {
    SdkError::Decode(what.into())
}

impl<'s> BamlValue<'s> {
    pub(crate) fn new(raw: RawValue, session: &'s Session) -> Self {
        Self { raw, session }
    }

    /// Carry the value past its session.
    pub fn detach(self) -> Detached {
        let detached = Detached {
            raw: self.raw,
            library: self.session.library(),
        };
        std::mem::forget(self);
        detached
    }

    pub fn raw(&self) -> RawValue {
        self.raw
    }

    /// Its type, without the handle bit.
    pub fn kind(&self) -> u64 {
        self.raw.tag & tag::TYPE
    }

    fn handle(&self) -> Result<u64, SdkError> {
        if self.raw.tag & tag::HANDLE == 0 {
            Err(decode(format!(
                "a scalar of tag {} where an object was expected",
                self.kind()
            )))
        } else {
            Ok(self.raw.payload)
        }
    }

    fn fns(&self) -> &'s Fns {
        self.session.fns()
    }

    pub fn is_null(&self) -> bool {
        self.raw.tag == tag::NULL
    }

    pub fn int(&self) -> Result<i64, SdkError> {
        (self.raw.tag == tag::INT)
            .then(|| self.raw.payload.cast_signed())
            .ok_or_else(|| decode(format!("tag {} where an int was expected", self.kind())))
    }

    pub fn float(&self) -> Result<f64, SdkError> {
        (self.raw.tag == tag::FLOAT)
            .then(|| f64::from_bits(self.raw.payload))
            .ok_or_else(|| decode(format!("tag {} where a float was expected", self.kind())))
    }

    pub fn bool(&self) -> Result<bool, SdkError> {
        (self.raw.tag == tag::BOOL)
            .then_some(self.raw.payload & 0xFF != 0)
            .ok_or_else(|| decode(format!("tag {} where a bool was expected", self.kind())))
    }

    pub fn is_string(&self) -> bool {
        matches!(self.kind(), tag::STRING | tag::STRING_SLICE) && self.raw.tag & tag::HANDLE != 0
    }

    pub fn string(&self) -> Result<String, SdkError> {
        let handle = self.handle()?;
        // SAFETY: under the lock; a buffer of its length.
        unsafe { text_from(|out, capacity| (self.fns().string_copy)(handle, out, capacity)) }
            .ok_or_else(|| decode("not a string"))
    }

    /// The value as an uncaught throw of it is written.
    pub fn describe(&self) -> String {
        let Ok(handle) = self.handle() else {
            return format!("{:?}", self.raw);
        };
        // SAFETY: as above.
        unsafe { text_from(|out, capacity| (self.fns().describe)(handle, out, capacity)) }
            .unwrap_or_default()
    }

    /// The number of its class or enum in the library's interface.
    pub fn class_of(&self) -> Option<u32> {
        let handle = self.handle().ok()?;
        // SAFETY: under the lock.
        u32::try_from(unsafe { (self.fns().class_of)(handle) }).ok()
    }

    /// Check that it is an instance of the class numbered `class`.
    pub fn expect_class(&self, class: u32) -> Result<(), SdkError> {
        if self.class_of() == Some(class) {
            Ok(())
        } else {
            Err(decode(format!("expected an instance of class {class}")))
        }
    }

    /// Field `index`, in declaration order.
    pub fn field(&self, index: u32) -> Result<BamlValue<'s>, SdkError> {
        let mut raw = RawValue { tag: 0, payload: 0 };
        // SAFETY: under the lock.
        if unsafe { (self.fns().field_get)(self.handle()?, index, &raw mut raw) } != 0 {
            return Err(self.session.refused());
        }
        Ok(self.session.owned(raw))
    }

    /// Its elements, when it is an array.
    pub fn elements(
        &self,
    ) -> Result<impl Iterator<Item = Result<BamlValue<'s>, SdkError>> + '_, SdkError> {
        let handle = self.handle()?;
        // SAFETY: under the lock.
        let length = u64::try_from(unsafe { (self.fns().array_len)(handle) })
            .map_err(|_| self.session.refused())?;
        Ok((0..length).map(move |index| {
            let mut raw = RawValue { tag: 0, payload: 0 };
            // SAFETY: under the lock.
            if unsafe { (self.fns().array_get)(handle, index, &raw mut raw) } != 0 {
                return Err(self.session.refused());
            }
            Ok(self.session.owned(raw))
        }))
    }

    /// Its entries in insertion order, when it is a map.
    pub fn entries(&self) -> Result<Vec<(BamlValue<'s>, BamlValue<'s>)>, SdkError> {
        let handle = self.handle()?;
        let mut entries = Vec::new();
        let mut cursor = 0;
        loop {
            let mut key = RawValue { tag: 0, payload: 0 };
            let mut value = RawValue { tag: 0, payload: 0 };
            // SAFETY: under the lock.
            let next =
                unsafe { (self.fns().map_next)(handle, cursor, &raw mut key, &raw mut value) };
            match next {
                -1 => return Ok(entries),
                next if next < 0 => return Err(self.session.refused()),
                next => {
                    entries.push((self.session.owned(key), self.session.owned(value)));
                    cursor = next.cast_unsigned();
                }
            }
        }
    }

    /// Its position in its enum, when it is an enum value.
    pub fn variant_index(&self) -> Result<u32, SdkError> {
        // SAFETY: under the lock.
        u32::try_from(unsafe { (self.fns().variant_index)(self.handle()?) })
            .map_err(|_| self.session.refused())
    }
}

/// Copying a Rust value into BAML memory.
pub trait ToBaml {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError>;
}

/// Copying a value out of BAML memory into a Rust value.
pub trait FromBaml: Sized {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError>;
}

impl ToBaml for i64 {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        Ok(session.int(*self))
    }
}

impl FromBaml for i64 {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
        value.int()
    }
}

impl ToBaml for f64 {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        Ok(session.float(*self))
    }
}

impl FromBaml for f64 {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
        value.float()
    }
}

impl ToBaml for bool {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        Ok(session.bool(*self))
    }
}

impl FromBaml for bool {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
        value.bool()
    }
}

impl ToBaml for String {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        session.string(self)
    }
}

impl ToBaml for str {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        session.string(self)
    }
}

impl FromBaml for String {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
        value.string()
    }
}

impl ToBaml for () {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        Ok(session.null())
    }
}

impl FromBaml for () {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
        if value.is_null() {
            Ok(())
        } else {
            Err(decode("a value where null was expected"))
        }
    }
}

impl<T: ToBaml> ToBaml for Option<T> {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        match self {
            Some(value) => value.to_baml(session),
            None => Ok(session.null()),
        }
    }
}

impl<T: FromBaml> FromBaml for Option<T> {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
        if value.is_null() {
            Ok(None)
        } else {
            T::from_baml(value).map(Some)
        }
    }
}

impl<T: ToBaml> ToBaml for Vec<T> {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        let elements = self
            .iter()
            .map(|element| element.to_baml(session))
            .collect::<Result<Vec<_>, _>>()?;
        session.array(elements)
    }
}

impl<T: FromBaml> FromBaml for Vec<T> {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
        value
            .elements()?
            .map(|element| T::from_baml(element?))
            .collect()
    }
}

impl<V: ToBaml> ToBaml for Map<String, V> {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        let entries = self
            .iter()
            .map(|(key, value)| Ok((session.string(key)?, value.to_baml(session)?)))
            .collect::<Result<Vec<_>, SdkError>>()?;
        session.map(entries)
    }
}

impl<V: FromBaml> FromBaml for Map<String, V> {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
        value
            .entries()?
            .into_iter()
            .map(|(key, value)| Ok((key.string()?, V::from_baml(value)?)))
            .collect()
    }
}

/// What a function that declares no `throws` cannot throw: anything it
/// throws arrives as [`Error::Runtime`].
impl FromBaml for std::convert::Infallible {
    fn from_baml(_value: BamlValue<'_>) -> Result<Self, SdkError> {
        Err(decode("a function that throws nothing threw"))
    }
}
