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


    // NOTE: the build_request api-key tests that lived here inspected the auth
    // header on `*_build_request`'s Request. That companion went away with the
    // legacy LLM path (credentials now resolve inside the provider's `invoke`,
    // at request time). Coverage moved to `_planv2/baml_src/live/`.
}
