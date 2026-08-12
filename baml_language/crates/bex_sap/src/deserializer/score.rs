use super::{
    deserialize_flags::{DeserializerConditions, Flag},
    types::ValueWithFlags,
};
use crate::sap_model::TypeIdent;

/// Lower is better
pub trait WithScore {
    fn score(&self) -> i32;
}

impl<N: TypeIdent> WithScore for Flag<'_, '_, '_, N> {
    fn score(&self) -> i32 {
        match self {
            Flag::InferedObject(_) => 0, // Don't penalize for this but instead handle it at the top level
            Flag::OptionalDefaultFromNoValue => 1,
            Flag::DefaultFromNoValue => 100,
            Flag::DefaultFromInProgress(_) => 0,
            Flag::DefaultButHadValue(_) => 110,
            Flag::ObjectFromFixedJson(_) => 0,
            Flag::ObjectFromMarkdown(s) => *s,
            Flag::DefaultButHadUnparseableValue(_) => 2,
            Flag::OptionalFieldError(_, _) => 10,
            Flag::ObjectToMap(_) => 1,
            Flag::ObjectToString(_) => 2,
            Flag::ObjectToPrimitive(_) => 2,
            Flag::ExtraKey(_, _) => 1,
            Flag::StrippedNonAlphaNumeric(_) => 3,
            Flag::SubstringMatch(_) => 2,
            Flag::ImpliedKey(_) => 2,
            Flag::JsonToString(_) => 2,
            Flag::SingleToArray => 1,
            // Parsing errors are bad.
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            Flag::ArrayItemParseError(x, _) => 1 + (*x as i32),
            Flag::MapKeyParseError(_x, _) => 1,
            Flag::MapValueParseError(_x, _) => 1,
            // Harmless to drop additional matches
            Flag::FirstMatch(_, _) => 1,
            // No penalty for picking an option from a union
            Flag::UnionMatch(_, _) => 0,
            Flag::StrMatchOneFromMany(values) =>
            {
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                values.iter().map(|(_, count)| *count as i32).sum::<i32>()
            }
            Flag::StringToInt(_) => 1,
            Flag::StringToBigint(_) => 1,
            Flag::StringToBool(_) => 1,
            Flag::StringToNull(_) => 1,
            Flag::StringToChar(_) => 1,
            Flag::StringToFloat(_) => 1,
            Flag::FloatToInt(_) => 1,
            Flag::FloatToBigint(_) => 1,
            Flag::NoFields(_) => 1,
            // No scores for incompleteness.
            Flag::Incomplete => 0,
            Flag::Pending => 0,
        }
    }
}

impl<T, N: TypeIdent> WithScore for ValueWithFlags<'_, '_, '_, T, N> {
    fn score(&self) -> i32 {
        self.meta.flags.score()
    }
}

impl<N: TypeIdent> WithScore for DeserializerConditions<'_, '_, '_, N> {
    fn score(&self) -> i32 {
        self.flags.iter().map(WithScore::score).sum()
    }
}

impl<N: TypeIdent> WithScore for Vec<Flag<'_, '_, '_, N>> {
    fn score(&self) -> i32 {
        self.iter().map(WithScore::score).sum()
    }
}
