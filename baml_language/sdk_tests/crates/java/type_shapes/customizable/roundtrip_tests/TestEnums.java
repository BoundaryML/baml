// Roundtrip coverage for `baml_sdk.enums` — enums + EnumVariant-as-type.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_enums.py — same test names, cases, inputs, assertions.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;

import baml_sdk.enums.Enums;
import baml_sdk.enums.Fns;
import baml_sdk.enums.Sentiment;
import org.junit.jupiter.api.Test;

class TestEnums {

    @Test
    void test_enums_pick_sentiment() {
        assertEquals(Sentiment.Positive, Fns.pick_sentiment(true));
        assertEquals(Sentiment.Negative, Fns.pick_sentiment(false));
    }

    @Test
    void test_enums_pick_positive() {
        assertEquals(Sentiment.Positive, Fns.pick_positive());
    }

    @Test
    void test_enums_round_trip_sentiment() {
        assertEquals(Sentiment.Negative, Fns.round_trip_sentiment(Sentiment.Negative));
    }

    @Test
    void test_enums_round_trip_sentiment_positive() {
        // java-port note: EnumVariant-as-type (`Sentiment.Positive` used as a
        // BAML *type*) drops the variant tag during TIR->codegen, same as
        // Python — the Java parameter/return type is just `Sentiment`.
        assertEquals(
                Sentiment.Positive, Fns.round_trip_sentiment_positive(Sentiment.Positive));
    }

    @Test
    void test_enums_round_trip_enums() {
        Enums e = new Enums(Sentiment.Positive, Sentiment.Positive);
        assertEquals(e, Fns.round_trip_enums(e));
    }
}
