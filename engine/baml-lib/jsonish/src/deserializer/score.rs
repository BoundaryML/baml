use super::{
    deserialize_flags::{DeserializerConditions, Flag},
    types::{BamlValueWithFlags, ValueWithFlags},
};

// Lower is better
pub trait WithScore {
    fn score(&self) -> i32;
}

impl WithScore for BamlValueWithFlags {
    fn score(&self) -> i32 {
        match self {
            BamlValueWithFlags::String(s) => s.score(),
            BamlValueWithFlags::Int(s) => s.score(),
            BamlValueWithFlags::Float(s) => s.score(),
            BamlValueWithFlags::Bool(s) => s.score(),
            BamlValueWithFlags::List(s, items) => {
                s.score() + 10 * items.iter().map(WithScore::score).sum::<i32>()
            }
            BamlValueWithFlags::Map(s, _) => s.score(),
            BamlValueWithFlags::Enum(_, s) => s.score(),
            BamlValueWithFlags::Class(_, s, kv) => {
                s.score() + 10 * kv.iter().map(|(_, v)| v.score()).sum::<i32>()
            }
            BamlValueWithFlags::Null(s) => s.score(),
            BamlValueWithFlags::Image(s) => s.score(),
        }
    }
}

impl WithScore for Flag {
    fn score(&self) -> i32 {
        match self {
            Flag::SingleFromMultiple => 1,
            Flag::OptionalDefaultFromNoValue => 1,
            Flag::DefaultFromNoValue => 100,
            Flag::DefaultButHadValue(_) => 110,
            Flag::ObjectFromFixedJson(_) => 0,
            Flag::ObjectFromMarkdown(s) => *s,
            Flag::DefaultButHadUnparseableValue(_) => 2,
            Flag::ObjectToString(_) => 2,
            Flag::ObjectToPrimitive(_) => 2,
            Flag::ExtraKey(_, _) => 1,
            Flag::StrippedNonAlphaNumeric(_) => 3,
            Flag::SubstringMatch(_) => 2,
            Flag::ImpliedKey(_) => 2,
            Flag::JsonToString(_) => 2,
            Flag::SingleToArray => 1,
            // Parsing errors are bad.
            Flag::ArrayItemParseError(x, _) => 1 + (*x as i32),
            // Harmless to drop additional matches
            Flag::FirstMatch(_, _) => 1,
            Flag::EnumOneFromMany(i) => i.into_iter().map(|(i, _)| *i as i32).sum::<i32>(),
            Flag::StringToBool(_) => 1,
            Flag::StringToNull(_) => 1,
            Flag::StringToChar(_) => 1,
            Flag::FloatToInt(_) => 1,
            Flag::NoFields(_) => 1,
        }
    }
}

impl<T> WithScore for ValueWithFlags<T> {
    fn score(&self) -> i32 {
        self.flags.score()
    }
}

impl WithScore for DeserializerConditions {
    fn score(&self) -> i32 {
        self.flags.iter().map(WithScore::score).sum()
    }
}
