// BAML `///` doc-comment lowering — port of python_pydantic2's
// docstrings_etc `test_main.py`.
//
// Python asserts on runtime `__doc__` strings; Swift has no runtime
// docstrings — `///` documentation lives in source and is consumed by
// the compiler/IDE. The Swift analog asserts the generated SOURCE
// carries the doc comments on the right declarations. (The Attributes:
// /Members: section rollup is a Python-docstring convention with no
// Swift equivalent; fields and variants get their own `///` lines
// instead.)
import XCTest
import Baml

final class TestMain: XCTestCase {
    private func generatedSource(_ file: String) throws -> String {
        // This test file is copied into generated/Tests/BamlTests/, so
        // the generated sources sit two directories up.
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()  // Tests/BamlTests
            .deletingLastPathComponent()  // Tests
            .deletingLastPathComponent()  // generated
            .appendingPathComponent("Sources/Baml/\(file)")
        return try String(contentsOf: url, encoding: .utf8)
    }

    func test_main_imports_symbols_reachable() {
        _ = Baml.docs.Doc.self
        _ = Baml.docs.Note.self
        _ = Baml.docs.Priority.self
        _ = Baml.docs.Sentiment.self
    }

    func test_main_class_doc_summary_present() throws {
        let source = try generatedSource("docs.swift")
        XCTAssertTrue(source.contains("/// A document with a title and an optional body."))
    }

    func test_main_multi_line_class_doc_preserved() throws {
        let source = try generatedSource("docs.swift")
        XCTAssertTrue(source.contains("/// A multi-line summary."))
        XCTAssertTrue(source.contains("/// Continuation line of the summary"))
    }

    func test_main_field_docs_attached() throws {
        let source = try generatedSource("docs.swift")
        XCTAssertTrue(source.contains("/// Title shown in lists and search results."))
        XCTAssertTrue(source.contains("/// Stable identifier — surfaces in URLs."))
    }

    func test_main_enum_and_variant_docs_attached() throws {
        let source = try generatedSource("docs.swift")
        XCTAssertTrue(source.contains("/// Sentiment labels surfaced by the model."))
        XCTAssertTrue(source.contains("/// Smiling face."))
        XCTAssertTrue(source.contains("/// Frowning face."))
    }
}
