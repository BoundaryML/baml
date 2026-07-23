// llm_functions codegen shape + shorthand-key wiring — port of
// python_pydantic2's llm test_main.py. Binding-existence tests are
// compile-time facts in Swift (touching the symbol IS the test);
// build_request tests exercise the real header pipeline.
import XCTest
import Foundation
import Baml
import BamlBridge

final class TestMain: XCTestCase {
    func test_types_and_bindings_reachable() {
        _ = Baml.lorem.Resume.self
        _ = Baml.lorem.StreamingDoc.self
        _ = Baml.ipsum.Sentiment.self
        _ = Baml.stream_types.lorem.Resume.self
        _ = Baml.stream_types.lorem.StreamingDoc.self
    }

    func test_ipsum_sentiment_enum_shape() {
        XCTAssertEqual(
            Set(Baml.ipsum.Sentiment.allCases.map(\.rawValue)),
            ["POSITIVE", "NEGATIVE", "NEUTRAL"]
        )
    }

    func test_extract_resume_build_request_includes_openai_api_key() throws {
        setenv("OPENAI_API_KEY", "sk-openai-shorthand-test", 1)
        defer { unsetenv("OPENAI_API_KEY") }
        let request = try Baml.lorem.ExtractResume_build_request(text: "Some resume text")
        let headers = Dictionary(
            uniqueKeysWithValues: request.headers.map { ($0.key.lowercased(), $0.value) }
        )
        XCTAssertEqual(headers["authorization"], "Bearer sk-openai-shorthand-test")
    }

    func test_streaming_extract_build_request_includes_openai_api_key() throws {
        setenv("OPENAI_API_KEY", "sk-openai-responses-test", 1)
        defer { unsetenv("OPENAI_API_KEY") }
        let request = try Baml.lorem.StreamingExtract_build_request(text: "Some text to summarize")
        let headers = Dictionary(
            uniqueKeysWithValues: request.headers.map { ($0.key.lowercased(), $0.value) }
        )
        XCTAssertEqual(headers["authorization"], "Bearer sk-openai-responses-test")
    }

    func test_classify_sentiment_build_request_includes_anthropic_api_key() throws {
        setenv("ANTHROPIC_API_KEY", "sk-ant-shorthand-test", 1)
        defer { unsetenv("ANTHROPIC_API_KEY") }
        let request = try Baml.ipsum.ClassifySentiment_build_request(text: "I love this!")
        let headers = Dictionary(
            uniqueKeysWithValues: request.headers.map { ($0.key.lowercased(), $0.value) }
        )
        XCTAssertEqual(headers["x-api-key"], "sk-ant-shorthand-test")
    }
}
