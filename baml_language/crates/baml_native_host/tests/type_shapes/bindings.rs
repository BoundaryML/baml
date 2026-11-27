// Bindings for the type-shapes fixture and the namespace `bridge_test`,
// written by hand as a generator would emit them over `baml_native_host`.
// The structs, enums and function signatures are the ones `sdkgen_rust`
// emits today (`sdk_tests/crates/rust/type_shapes/generated`). The
// numbers are from `native_pack --lib <sources> <out> <exports...>
// --describe` with the exports `tests/type_shapes.rs` packs.

#![allow(
    dead_code,
    unreachable_pub,
    clippy::struct_field_names,
    clippy::wildcard_imports,
    reason = "bindings for every export, public as generated code is; each test calls some"
)]

pub use baml_native_host::Map;

/// The interface hash of the library these bindings are for.
pub const INTERFACE_HASH: u64 = 0x6933_8fd0_293f_2b04;

pub mod _runtime {
    use std::sync::OnceLock;

    use baml_native_host::{Library, SdkError, futures};

    static LIBRARY: OnceLock<&'static Library> = OnceLock::new();

    /// Load the library at `path`, refusing one generated for another
    /// version of the program.
    pub fn init(path: &str) -> Result<&'static Library, SdkError> {
        let library = futures::load(path, super::INTERFACE_HASH)?;
        Ok(*LIBRARY.get_or_init(|| library))
    }

    pub fn library() -> Result<&'static Library, SdkError> {
        LIBRARY
            .get()
            .copied()
            .ok_or_else(|| SdkError::Load("call `_runtime::init` first".to_owned()))
    }
}

mod numbers {
    pub const FN_ADD: u32 = 0;
    pub const FN_LOOKUP: u32 = 1;
    pub const FN_ROUND_TRIP_POINTS: u32 = 2;
    pub const FN_MAKE_POINTS: u32 = 3;
    pub const FN_COUNT_POINTS: u32 = 4;
    pub const FN_MAKE_INTS: u32 = 5;
    pub const FN_COUNT_INTS: u32 = 6;
    pub const FN_NAP: u32 = 7;
    pub const FN_ENV_OR: u32 = 8;
    pub const FN_MAKE_FOO: u32 = 9;
    pub const FN_ROUND_TRIP_FOO: u32 = 10;
    pub const FN_MAKE_OUTER: u32 = 11;
    pub const FN_ROUND_TRIP_INNER: u32 = 12;
    pub const FN_ROUND_TRIP_OUTER: u32 = 13;
    pub const FN_ROUND_TRIP_COMPLEX_PROFILE: u32 = 14;
    pub const FN_PICK_SENTIMENT: u32 = 15;
    pub const FN_ROUND_TRIP_SENTIMENT: u32 = 16;
    pub const FN_ROUND_TRIP_ENUMS: u32 = 17;
    pub const FN_ROUND_TRIP_INTS: u32 = 18;
    pub const FN_ROUND_TRIP_OPTIONAL_STRINGS: u32 = 19;
    pub const FN_ROUND_TRIP_UNION_LIST: u32 = 20;
    pub const FN_ROUND_TRIP_LIST_CONTAINER: u32 = 21;
    pub const FN_ROUND_TRIP_SIMPLE_MAP: u32 = 22;
    pub const FN_ROUND_TRIP_ENUM_KEYED_MAP: u32 = 23;
    pub const FN_ROUND_TRIP_LIST_VALUED_MAP: u32 = 24;
    pub const FN_ROUND_TRIP_MAP_CONTAINER: u32 = 25;
    pub const FN_ROUND_TRIP_OPTIONAL_INT: u32 = 26;
    pub const FN_ROUND_TRIP_OPTIONAL_RESUME: u32 = 27;
    pub const FN_ROUND_TRIP_OPTIONAL_UNION: u32 = 28;
    pub const FN_ROUND_TRIP_OPTIONAL_CONTAINER: u32 = 29;
    pub const FN_ROUND_TRIP_NULL_TO_END: u32 = 30;
    pub const FN_ROUND_TRIP_UNION_CONTAINER: u32 = 31;
    pub const FN_ROUND_TRIP_STR_OR_INT_LIST: u32 = 32;
    pub const FN_ROUND_TRIP_OPTIONAL_PLUS_NULL: u32 = 33;
    pub const FN_ROUND_TRIP_INT: u32 = 34;
    pub const FN_ROUND_TRIP_FLOAT: u32 = 35;
    pub const FN_ROUND_TRIP_STRING: u32 = 36;
    pub const FN_ROUND_TRIP_BOOL: u32 = 37;
    pub const FN_RETURN_NULL: u32 = 38;
    pub const FN_TWO_LISTS: u32 = 39;
    pub const FN_FAIL_QUIETLY: u32 = 40;

    pub const LOOKUP_FAILED: u32 = 0;
    pub const POINT: u32 = 1;
    pub const FOO: u32 = 2;
    pub const OUTER: u32 = 3;
    pub const INNER: u32 = 4;
    pub const COMPLEX_PROFILE: u32 = 5;
    pub const ACCOUNT_TIER: u32 = 6;
    pub const PROFILE_OWNER: u32 = 7;
    pub const CONTACT_METHOD: u32 = 8;
    pub const POSTAL_ADDRESS: u32 = 9;
    pub const GEO_POINT: u32 = 10;
    pub const INVOICE: u32 = 11;
    pub const LINE_ITEM: u32 = 12;
    pub const CARD_PAYMENT: u32 = 13;
    pub const WIRE_PAYMENT: u32 = 14;
    pub const AUDIT_EVENT: u32 = 15;
    pub const SENTIMENT: u32 = 16;
    pub const ENUMS: u32 = 17;
    pub const LIST_CONTAINER: u32 = 18;
    pub const MAPS_RESUME: u32 = 19;
    pub const MAP_CONTAINER: u32 = 20;
    pub const OPTIONAL_RESUME: u32 = 21;
    pub const OPTIONAL_CONTAINER: u32 = 22;
    pub const UNION_CONTAINER: u32 = 23;
    pub const T: u32 = 24;
}

/// A class: a struct, copied into an instance field by field and out of
/// one the same way.
macro_rules! class {
    ($(#[$meta:meta])* $name:ident = $number:expr; $($field:ident: $ty:ty = $slot:expr),* $(,)?) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq)]
        pub struct $name {
            $(pub $field: $ty,)*
        }
        impl ::baml_native_host::ToBaml for $name {
            fn to_baml<'s>(
                &self,
                session: &'s ::baml_native_host::Session,
            ) -> ::std::result::Result<::baml_native_host::BamlValue<'s>, ::baml_native_host::SdkError> {
                session.instance($number, ::std::vec![
                    $(::baml_native_host::ToBaml::to_baml(&self.$field, session)?,)*
                ])
            }
        }
        impl ::baml_native_host::FromBaml for $name {
            fn from_baml(
                value: ::baml_native_host::BamlValue<'_>,
            ) -> ::std::result::Result<Self, ::baml_native_host::SdkError> {
                value.expect_class($number)?;
                ::std::result::Result::Ok(Self {
                    $($field: ::baml_native_host::FromBaml::from_baml(value.field($slot)?)?,)*
                })
            }
        }
    };
}

/// An enum: a variant crosses as its position.
macro_rules! enumeration {
    ($name:ident = $number:expr; $($variant:ident = $index:expr),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant,)*
        }
        impl ::baml_native_host::ToBaml for $name {
            fn to_baml<'s>(
                &self,
                session: &'s ::baml_native_host::Session,
            ) -> ::std::result::Result<::baml_native_host::BamlValue<'s>, ::baml_native_host::SdkError> {
                session.variant($number, match self { $(Self::$variant => $index,)* })
            }
        }
        impl ::baml_native_host::FromBaml for $name {
            fn from_baml(
                value: ::baml_native_host::BamlValue<'_>,
            ) -> ::std::result::Result<Self, ::baml_native_host::SdkError> {
                value.expect_class($number)?;
                match value.variant_index()? {
                    $($index => ::std::result::Result::Ok(Self::$variant),)*
                    other => ::std::result::Result::Err(::baml_native_host::SdkError::Decode(
                        ::std::format!("no variant {other} of {}", ::std::stringify!($name)),
                    )),
                }
            }
        }
    };
}

/// A union of string literals: a unit variant per literal, crossing as its
/// text.
macro_rules! literals {
    ($name:ident; $($variant:ident = $text:literal),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant,)*
        }
        impl ::baml_native_host::ToBaml for $name {
            fn to_baml<'s>(
                &self,
                session: &'s ::baml_native_host::Session,
            ) -> ::std::result::Result<::baml_native_host::BamlValue<'s>, ::baml_native_host::SdkError> {
                session.string(match self { $(Self::$variant => $text,)* })
            }
        }
        impl ::baml_native_host::FromBaml for $name {
            fn from_baml(
                value: ::baml_native_host::BamlValue<'_>,
            ) -> ::std::result::Result<Self, ::baml_native_host::SdkError> {
                match value.string()?.as_str() {
                    $($text => ::std::result::Result::Ok(Self::$variant),)*
                    other => ::std::result::Result::Err(::baml_native_host::SdkError::Decode(
                        ::std::format!("{other:?} is no arm of {}", ::std::stringify!($name)),
                    )),
                }
            }
        }
    };
}

/// One exported function, with the signature `sdkgen_rust` emits for its
/// async form. Each argument says how it is taken: by `value`, as
/// `(borrow T)` from an `impl Borrow<T>`, or as `(into T)` from an
/// `impl Into<T>`.
macro_rules! function {
    ($name:ident($($arg:ident: $ty:ty as $kind:tt),*) -> $result:ty, $throws:ty = $number:expr) => {
        pub async fn $name($($arg: $ty),*) -> ::std::result::Result<$result, ::baml_native_host::Error<$throws>> {
            let library = crate::bindings::_runtime::library()?;
            ::baml_native_host::futures::call::<$result, $throws>(library, $number, |session| {
                    let _ = session;
                    ::std::result::Result::Ok(::std::vec![$(argument!($kind, $arg, session)),*])
                })
                .await
        }
    };
}

macro_rules! argument {
    (value, $arg:ident, $session:ident) => {
        ::baml_native_host::ToBaml::to_baml(&$arg, $session)?
    };
    ((borrow $t:ty), $arg:ident, $session:ident) => {
        ::baml_native_host::ToBaml::to_baml(::std::borrow::Borrow::<$t>::borrow(&$arg), $session)?
    };
    ((into $t:ty), $arg:ident, $session:ident) => {
        ::baml_native_host::ToBaml::to_baml(&::std::convert::Into::<$t>::into($arg), $session)?
    };
}

use numbers::*;

class!(Foo = FOO; v: i64 = 0);
function!(make_foo(v: i64 as value) -> Foo, ::std::convert::Infallible = FN_MAKE_FOO);
function!(round_trip_foo(f: impl ::std::borrow::Borrow<Foo> as (borrow Foo)) -> Foo, ::std::convert::Infallible = FN_ROUND_TRIP_FOO);

pub mod bridge_test {
    use super::numbers::*;

    class!(LookupFailed = LOOKUP_FAILED; key: String = 0, code: i64 = 1);
    class!(Point = POINT; x: i64 = 0, y: i64 = 1);
    function!(add(a: i64 as value, b: i64 as value) -> i64, ::std::convert::Infallible = FN_ADD);
    function!(lookup(key: String as value) -> String, LookupFailed = FN_LOOKUP);
    function!(round_trip_points(points: Vec<Point> as value) -> Vec<Point>, ::std::convert::Infallible = FN_ROUND_TRIP_POINTS);
    function!(make_points(n: i64 as value) -> Vec<Point>, ::std::convert::Infallible = FN_MAKE_POINTS);
    function!(count_points(points: Vec<Point> as value) -> i64, ::std::convert::Infallible = FN_COUNT_POINTS);
    function!(make_ints(n: i64 as value) -> Vec<i64>, ::std::convert::Infallible = FN_MAKE_INTS);
    function!(count_ints(xs: Vec<i64> as value) -> i64, ::std::convert::Infallible = FN_COUNT_INTS);
    function!(nap(milliseconds: i64 as value) -> i64, ::std::convert::Infallible = FN_NAP);
    function!(env_or(name: String as value, fallback: String as value) -> String, ::std::convert::Infallible = FN_ENV_OR);
    function!(two_lists(a: Vec<i64> as value, b: Vec<i64> as value) -> i64, ::std::convert::Infallible = FN_TWO_LISTS);
    function!(fail_quietly() -> i64, ::std::convert::Infallible = FN_FAIL_QUIETLY);
}

pub mod class_refs {
    use super::numbers::*;

    class!(Inner = INNER; value: i64 = 0);
    class!(Outer = OUTER; inner: Inner = 0);
    function!(make_outer(value: i64 as value) -> Outer, ::std::convert::Infallible = FN_MAKE_OUTER);
    function!(round_trip_inner(i: impl ::std::borrow::Borrow<Inner> as (borrow Inner)) -> Inner, ::std::convert::Infallible = FN_ROUND_TRIP_INNER);
    function!(round_trip_outer(o: impl ::std::borrow::Borrow<Outer> as (borrow Outer)) -> Outer, ::std::convert::Infallible = FN_ROUND_TRIP_OUTER);
}

pub mod complex_models {
    use baml_native_host::{BamlValue, FromBaml, SdkError, Session, ToBaml, tag};

    use super::{Map, numbers::*};

    enumeration!(AccountTier = ACCOUNT_TIER; Free = 0, Pro = 1, Enterprise = 2);
    class!(GeoPoint = GEO_POINT; lat: f64 = 0, lng: f64 = 1);
    class!(PostalAddress = POSTAL_ADDRESS;
        line1: String = 0,
        line2: Option<String> = 1,
        city: String = 2,
        region: String = 3,
        postal_code: String = 4,
        location: Option<GeoPoint> = 5,
    );
    class!(ContactMethod = CONTACT_METHOD; label: String = 0, value: String = 1, verified: bool = 2);
    class!(ProfileOwner = PROFILE_OWNER;
        name: String = 0,
        primary_contact: ContactMethod = 1,
        backup_contacts: Vec<ContactMethod> = 2,
    );
    class!(CardPayment = CARD_PAYMENT; brand: String = 0, last4: String = 1, billing_address: PostalAddress = 2);
    class!(WirePayment = WIRE_PAYMENT; bank_name: String = 0, routing_code: String = 1, reference: Option<String> = 2);
    class!(LineItem = LINE_ITEM;
        sku: String = 0,
        quantity: i64 = 1,
        unit_price: f64 = 2,
        tags: Vec<String> = 3,
        attributes: Map<String, String> = 4,
    );
    literals!(DraftOrSentOrPaid; Draft = "draft", Sent = "sent", Paid = "paid");
    literals!(CreatedOrUpdatedOrApproved; Created = "created", Updated = "updated", Approved = "approved");
    class!(Invoice = INVOICE;
        id: String = 0,
        status: DraftOrSentOrPaid = 1,
        items: Vec<LineItem> = 2,
        payment: Option<CardPaymentOrWirePayment> = 3,
        notes: Option<String> = 4,
    );
    class!(AuditEvent = AUDIT_EVENT; actor: String = 0, action: CreatedOrUpdatedOrApproved = 1, context: Map<String, String> = 2);
    class!(ComplexProfile = COMPLEX_PROFILE;
        id: String = 0,
        tier: AccountTier = 1,
        owner: ProfileOwner = 2,
        addresses: Vec<PostalAddress> = 3,
        invoices: Vec<Invoice> = 4,
        audit_trail: Vec<AuditEvent> = 5,
        metadata: Map<String, String> = 6,
        featured: Option<InvoiceOrPostalAddressOrString> = 7,
        flags: Vec<IntOrStringOrBool> = 8,
    );

    #[derive(Debug, Clone, PartialEq)]
    pub enum CardPaymentOrWirePayment {
        CardPayment(CardPayment),
        WirePayment(WirePayment),
    }
    impl ToBaml for CardPaymentOrWirePayment {
        fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
            match self {
                Self::CardPayment(value) => value.to_baml(session),
                Self::WirePayment(value) => value.to_baml(session),
            }
        }
    }
    impl FromBaml for CardPaymentOrWirePayment {
        fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
            match value.class_of() {
                Some(CARD_PAYMENT) => FromBaml::from_baml(value).map(Self::CardPayment),
                Some(WIRE_PAYMENT) => FromBaml::from_baml(value).map(Self::WirePayment),
                _ => Err(SdkError::Decode(
                    "no arm of CardPaymentOrWirePayment".to_owned(),
                )),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum InvoiceOrPostalAddressOrString {
        Invoice(Invoice),
        PostalAddress(PostalAddress),
        String(String),
    }
    impl ToBaml for InvoiceOrPostalAddressOrString {
        fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
            match self {
                Self::Invoice(value) => value.to_baml(session),
                Self::PostalAddress(value) => value.to_baml(session),
                Self::String(value) => value.to_baml(session),
            }
        }
    }
    impl FromBaml for InvoiceOrPostalAddressOrString {
        fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
            if value.is_string() {
                return value.string().map(Self::String);
            }
            match value.class_of() {
                Some(INVOICE) => FromBaml::from_baml(value).map(Self::Invoice),
                Some(POSTAL_ADDRESS) => FromBaml::from_baml(value).map(Self::PostalAddress),
                _ => Err(SdkError::Decode(
                    "no arm of InvoiceOrPostalAddressOrString".to_owned(),
                )),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum IntOrStringOrBool {
        Int(i64),
        String(String),
        Bool(bool),
    }
    impl ToBaml for IntOrStringOrBool {
        fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
            match self {
                Self::Int(value) => value.to_baml(session),
                Self::String(value) => value.to_baml(session),
                Self::Bool(value) => value.to_baml(session),
            }
        }
    }
    impl FromBaml for IntOrStringOrBool {
        fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
            match value.kind() {
                tag::INT => value.int().map(Self::Int),
                tag::BOOL => value.bool().map(Self::Bool),
                _ => value.string().map(Self::String),
            }
        }
    }

    function!(round_trip_complex_profile(profile: impl ::std::borrow::Borrow<ComplexProfile> as (borrow ComplexProfile)) -> ComplexProfile, ::std::convert::Infallible = FN_ROUND_TRIP_COMPLEX_PROFILE);
}

pub mod enums {
    use super::numbers::*;

    enumeration!(Sentiment = SENTIMENT; Positive = 0, Negative = 1);
    class!(Enums = ENUMS; bare_enum: Sentiment = 0, variant_as_type: Sentiment = 1);
    function!(pick_sentiment(b: bool as value) -> Sentiment, ::std::convert::Infallible = FN_PICK_SENTIMENT);
    function!(round_trip_sentiment(s: Sentiment as value) -> Sentiment, ::std::convert::Infallible = FN_ROUND_TRIP_SENTIMENT);
    function!(round_trip_enums(e: impl ::std::borrow::Borrow<Enums> as (borrow Enums)) -> Enums, ::std::convert::Infallible = FN_ROUND_TRIP_ENUMS);
}

/// `int | string`, as several namespaces have it.
macro_rules! int_or_string {
    () => {
        #[derive(Debug, Clone, PartialEq)]
        pub enum IntOrString {
            Int(i64),
            String(String),
        }
        impl ::baml_native_host::ToBaml for IntOrString {
            fn to_baml<'s>(
                &self,
                session: &'s ::baml_native_host::Session,
            ) -> Result<::baml_native_host::BamlValue<'s>, ::baml_native_host::SdkError> {
                match self {
                    Self::Int(value) => ::baml_native_host::ToBaml::to_baml(value, session),
                    Self::String(value) => ::baml_native_host::ToBaml::to_baml(value, session),
                }
            }
        }
        impl ::baml_native_host::FromBaml for IntOrString {
            fn from_baml(
                value: ::baml_native_host::BamlValue<'_>,
            ) -> Result<Self, ::baml_native_host::SdkError> {
                if value.kind() == ::baml_native_host::tag::INT {
                    value.int().map(Self::Int)
                } else {
                    value.string().map(Self::String)
                }
            }
        }
    };
}

pub mod lists {
    use super::numbers::*;

    int_or_string!();
    class!(ListContainer = LIST_CONTAINER;
        ints: Vec<i64> = 0,
        optional_strings: Vec<Option<String>> = 1,
        union_list: Vec<IntOrString> = 2,
    );
    function!(round_trip_ints(xs: Vec<i64> as value) -> Vec<i64>, ::std::convert::Infallible = FN_ROUND_TRIP_INTS);
    function!(round_trip_optional_strings(xs: Vec<Option<String>> as value) -> Vec<Option<String>>, ::std::convert::Infallible = FN_ROUND_TRIP_OPTIONAL_STRINGS);
    function!(round_trip_union_list(xs: Vec<IntOrString> as value) -> Vec<IntOrString>, ::std::convert::Infallible = FN_ROUND_TRIP_UNION_LIST);
    function!(round_trip_list_container(c: impl ::std::borrow::Borrow<ListContainer> as (borrow ListContainer)) -> ListContainer, ::std::convert::Infallible = FN_ROUND_TRIP_LIST_CONTAINER);
}

pub mod maps {
    use super::{Map, numbers::*};

    class!(Resume = MAPS_RESUME; name: String = 0);
    class!(MapContainer = MAP_CONTAINER;
        simple: Map<String, i64> = 0,
        enum_keyed: Map<String, Resume> = 1,
        list_valued: Map<String, Vec<i64>> = 2,
    );
    function!(round_trip_simple_map(m: Map<String, i64> as value) -> Map<String, i64>, ::std::convert::Infallible = FN_ROUND_TRIP_SIMPLE_MAP);
    function!(round_trip_enum_keyed_map(m: Map<String, Resume> as value) -> Map<String, Resume>, ::std::convert::Infallible = FN_ROUND_TRIP_ENUM_KEYED_MAP);
    function!(round_trip_list_valued_map(m: Map<String, Vec<i64>> as value) -> Map<String, Vec<i64>>, ::std::convert::Infallible = FN_ROUND_TRIP_LIST_VALUED_MAP);
    function!(round_trip_map_container(c: impl ::std::borrow::Borrow<MapContainer> as (borrow MapContainer)) -> MapContainer, ::std::convert::Infallible = FN_ROUND_TRIP_MAP_CONTAINER);
}

pub mod optional {
    use super::numbers::*;

    int_or_string!();
    class!(Resume = OPTIONAL_RESUME; name: String = 0);
    class!(OptionalContainer = OPTIONAL_CONTAINER;
        optional_int: Option<i64> = 0,
        optional_class: Option<Resume> = 1,
        optional_union: Option<IntOrString> = 2,
    );
    function!(round_trip_optional_int(x: Option<i64> as value) -> Option<i64>, ::std::convert::Infallible = FN_ROUND_TRIP_OPTIONAL_INT);
    function!(round_trip_optional_resume(r: Option<Resume> as value) -> Option<Resume>, ::std::convert::Infallible = FN_ROUND_TRIP_OPTIONAL_RESUME);
    function!(round_trip_optional_union(u: impl Into<Option<IntOrString>> as (into Option<IntOrString>)) -> Option<IntOrString>, ::std::convert::Infallible = FN_ROUND_TRIP_OPTIONAL_UNION);
    function!(round_trip_optional_container(c: impl ::std::borrow::Borrow<OptionalContainer> as (borrow OptionalContainer)) -> OptionalContainer, ::std::convert::Infallible = FN_ROUND_TRIP_OPTIONAL_CONTAINER);
}

pub mod unions {
    use baml_native_host::{BamlValue, FromBaml, SdkError, Session, ToBaml, tag};

    use super::numbers::*;

    int_or_string!();
    class!(T = T; v: i64 = 0);

    #[derive(Debug, Clone, PartialEq)]
    pub enum TOrString {
        T(T),
        String(String),
    }
    impl ToBaml for TOrString {
        fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
            match self {
                Self::T(value) => value.to_baml(session),
                Self::String(value) => value.to_baml(session),
            }
        }
    }
    impl FromBaml for TOrString {
        fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
            if value.is_string() {
                value.string().map(Self::String)
            } else {
                FromBaml::from_baml(value).map(Self::T)
            }
        }
    }

    /// `string[] | int[]`. An array does not carry its element type, so
    /// an empty one reads as the first arm.
    #[derive(Debug, Clone, PartialEq)]
    pub enum StringListOrIntList {
        StringList(Vec<String>),
        IntList(Vec<i64>),
    }
    impl ToBaml for StringListOrIntList {
        fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
            match self {
                Self::StringList(value) => value.to_baml(session),
                Self::IntList(value) => value.to_baml(session),
            }
        }
    }
    impl FromBaml for StringListOrIntList {
        fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
            let first_is_int = match value.elements()?.next() {
                Some(first) => first?.kind() == tag::INT,
                None => false,
            };
            if first_is_int {
                FromBaml::from_baml(value).map(Self::IntList)
            } else {
                FromBaml::from_baml(value).map(Self::StringList)
            }
        }
    }

    class!(UnionContainer = UNION_CONTAINER;
        null_to_end: Option<IntOrString> = 0,
        dedup: IntOrString = 1,
        singleton_unwrap: i64 = 2,
        optional_plus_null: Option<TOrString> = 3,
    );
    function!(round_trip_null_to_end(u: impl Into<Option<IntOrString>> as (into Option<IntOrString>)) -> Option<IntOrString>, ::std::convert::Infallible = FN_ROUND_TRIP_NULL_TO_END);
    function!(round_trip_union_container(c: impl ::std::borrow::Borrow<UnionContainer> as (borrow UnionContainer)) -> UnionContainer, ::std::convert::Infallible = FN_ROUND_TRIP_UNION_CONTAINER);
    function!(round_trip_str_or_int_list(x: impl Into<StringListOrIntList> as (into StringListOrIntList)) -> StringListOrIntList, ::std::convert::Infallible = FN_ROUND_TRIP_STR_OR_INT_LIST);
    function!(round_trip_optional_plus_null(u: impl Into<Option<TOrString>> as (into Option<TOrString>)) -> Option<TOrString>, ::std::convert::Infallible = FN_ROUND_TRIP_OPTIONAL_PLUS_NULL);
}

pub mod primitives {
    use super::numbers::*;

    function!(round_trip_int(x: i64 as value) -> i64, ::std::convert::Infallible = FN_ROUND_TRIP_INT);
    function!(round_trip_float(x: f64 as value) -> f64, ::std::convert::Infallible = FN_ROUND_TRIP_FLOAT);
    function!(round_trip_string(x: String as value) -> String, ::std::convert::Infallible = FN_ROUND_TRIP_STRING);
    function!(round_trip_bool(x: bool as value) -> bool, ::std::convert::Infallible = FN_ROUND_TRIP_BOOL);
    function!(return_null() -> (), ::std::convert::Infallible = FN_RETURN_NULL);
}
