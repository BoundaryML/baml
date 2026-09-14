// llm_functions codegen shape + shorthand-key wiring — port of
// python_pydantic2's llm test_main.py. Binding-existence tests are
// compile-time facts in Swift (touching the symbol IS the test);
// build_request tests exercise the real header pipeline.
import XCTest
import Foundation
import Baml
import BamlBridge

final class TestMain: XCTestCase {
    func test_main_types_and_bindings_reachable() {
        _ = Baml.lorem.Resume.self
        _ = Baml.lorem.StreamingDoc.self
        _ = Baml.ipsum.Sentiment.self
        _ = Baml.stream_types.lorem.Resume.self
        _ = Baml.stream_types.lorem.StreamingDoc.self
    }

    func test_main_ipsum_sentiment_enum_shape() {
        XCTAssertEqual(
            Set(Baml.ipsum.Sentiment.allCases.map(\.rawValue)),
            ["POSITIVE", "NEGATIVE", "NEUTRAL"]
        )
    }


    // NOTE: the legacy `*_build_request` binding is gone. Request previews now
    // start with `Fn_spec(...)` and call `buildRequest()` on that value.
}
