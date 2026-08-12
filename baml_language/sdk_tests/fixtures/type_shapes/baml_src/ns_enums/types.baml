// Enum + EnumVariant-as-type rows from 18a §"Type shapes".
//   - `Sentiment`           → `Sentiment` (Enum(qtn))
//   - `Sentiment.Positive`  → `Sentiment` (EnumVariant — variant tag dropped
//                                          during TIR→codegen)
//
// Field-bearing class is what makes it into Python; functions exercise the
// HIR side and aren't emitted (see 18b).

enum Sentiment {
  Positive,
  Negative,
}

class Enums {
  bare_enum Sentiment
  variant_as_type Sentiment.Positive
}

function pick_sentiment(b: bool) -> Sentiment {
  if (b) { Sentiment.Positive } else { Sentiment.Negative }
}

function pick_positive() -> Sentiment.Positive {
  Sentiment.Positive
}

function round_trip_sentiment(s: Sentiment) -> Sentiment {
  s
}

function round_trip_sentiment_positive(s: Sentiment.Positive) -> Sentiment.Positive {
  s
}

function round_trip_enums(e: Enums) -> Enums {
  e
}
