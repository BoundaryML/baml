// Throws-contract Raises: doc-comment coverage. Port of
// function_calls/customizable/test_raises.py: Python inspects runtime
// __doc__ / .pyi source; the C++ surface is the generated header's
// /// Raises: doc comments, asserted on the header text. One shared doc
// block precedes the sync + _async sibling pair (unlike Python's per-binding
// docstrings).
#include <fstream>
#include <sstream>
#include <string>

#include <baml_test.hpp>

static std::string header_text() {
    std::ifstream in("baml_sdk/include/baml_sdk.hpp");
    if (!in) {
        baml_test::fail("cannot open baml_sdk/include/baml_sdk.hpp");
    }
    std::ostringstream out;
    out << in.rdbuf();
    return out.str();
}

static size_t find_or_fail(const std::string& text, const std::string& needle) {
    const size_t pos = text.find(needle);
    if (pos == std::string::npos) {
        baml_test::fail("header missing: " + needle);
    }
    return pos;
}

BAML_TEST(union_throws_lists_all_names) {
    const std::string text = header_text();
    const size_t raises = find_or_fail(text, "/// Raises: ParseError, TimeoutError");
    // The shared doc block immediately precedes the sync + async pair.
    const size_t decl = text.find("std::string LoadDoc(", raises);
    BAML_ASSERT(decl != std::string::npos && decl - raises < 200);
}

BAML_TEST(async_sibling_shares_raises_block) {
    const std::string text = header_text();
    const size_t raises = find_or_fail(text, "/// Raises: ParseError, TimeoutError");
    const size_t async_decl = text.find("::baml::Future<std::string> LoadDoc_async(", raises);
    BAML_ASSERT(async_decl != std::string::npos && async_decl - raises < 300);
}

BAML_TEST(single_throws) {
    const std::string text = header_text();
    const size_t raises = text.find("/// Raises: ParseError\n");
    BAML_ASSERT(raises != std::string::npos);
    const size_t decl = text.find("Reparse(", raises);
    BAML_ASSERT(decl != std::string::npos);
}

BAML_TEST(summary_precedes_raises_block) {
    const std::string text = header_text();
    const size_t summary = find_or_fail(text, "/// Load a document from a path.");
    const size_t raises = find_or_fail(text, "/// Raises: ParseError, TimeoutError");
    BAML_ASSERT(summary < raises && raises - summary < 200);
}

BAML_TEST(inferred_contract_without_clause_still_raises) {
    const std::string text = header_text();
    const size_t decl = find_or_fail(text, "std::string InferredThrow(");
    const std::string before = text.substr(decl < 300 ? 0 : decl - 300, std::min(decl, size_t{300}));
    BAML_ASSERT(before.find("/// Raises: ParseError") != std::string::npos);
}

BAML_TEST(non_throwing_function_has_no_raises_block) {
    const std::string text = header_text();
    const size_t decl = find_or_fail(text, "int64_t PureLen(");
    const std::string before = text.substr(decl < 150 ? 0 : decl - 150, std::min(decl, size_t{150}));
    BAML_ASSERT(before.find("Raises:") == std::string::npos);
}

BAML_TEST(method_raises_blocks_in_struct) {
    const std::string text = header_text();
    const size_t struct_start = find_or_fail(text, "struct DocLoader {");
    const size_t struct_end = text.find("\n};", struct_start);
    BAML_ASSERT(struct_end != std::string::npos);
    const std::string body = text.substr(struct_start, struct_end - struct_start);
    BAML_ASSERT(body.find("/// Raises:") != std::string::npos);
    BAML_ASSERT(body.find("ParseError") != std::string::npos);
    BAML_ASSERT(body.find("TimeoutError") != std::string::npos);
}
