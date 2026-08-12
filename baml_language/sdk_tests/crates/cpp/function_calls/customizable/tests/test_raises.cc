// Throws-contract `/// Raises:` doc-comment coverage. Port of the
// free-function subset of function_calls/customizable/test_raises.py:
// Python asserts on runtime __doc__; the C++ surface is the generated
// header's `/// Raises:` lines (unqualified type names), scraped from the
// header text. The method/.pyi sub-cases are post-step-8.
#include <baml_sdk.h>
#include <baml_test.h>

#include <fstream>
#include <sstream>
#include <string>

static std::string header_text() {
  std::ifstream in("baml_sdk/include/baml_sdk.h");
  if (!in) {
    baml_test::fail("cannot open baml_sdk/include/baml_sdk.h");
  }
  std::ostringstream out;
  out << in.rdbuf();
  return out.str();
}

BAML_TEST(raises_imports_symbols_reachable) {
  // Port of python's test_imports: every raises_test symbol is reachable
  // at its qualified path. Compile-level analog of the import assertions -
  // the header scrapes below are namespace-blind, so without this a
  // wrong-namespace regression would pass them.
  (void)sizeof(baml_sdk::raises_test::DocLoader);
  (void)&baml_sdk::raises_test::InferredThrow;
  (void)&baml_sdk::raises_test::LoadDoc;
  (void)&baml_sdk::raises_test::PureLen;
  (void)&baml_sdk::raises_test::Reparse;
}

BAML_TEST(raises_union_throws_lists_all_names) {
  // A multi-member throws union lists every member, unqualified.
  BAML_ASSERT(header_text().find("/// Raises: ParseError, TimeoutError\n"
                                 "std::string LoadDoc(") != std::string::npos);
}

BAML_TEST(raises_single_throws) {
  BAML_ASSERT(header_text().find("/// Raises: ParseError\n"
                                 "std::string Reparse(") != std::string::npos);
}

BAML_TEST(raises_summary_precedes_raises_block) {
  BAML_ASSERT(header_text().find("/// Load a document from a path.\n"
                                 "/// Raises: ParseError, TimeoutError\n"
                                 "std::string LoadDoc(") != std::string::npos);
}

BAML_TEST(raises_inferred_contract_without_clause_still_raises) {
  // No written `throws` clause, but the body throws ParseError -- the
  // inferred contract (callable_throws) still surfaces a Raises line.
  BAML_ASSERT(header_text().find("/// Raises: ParseError\n"
                                 "std::string InferredThrow(") !=
              std::string::npos);
}

BAML_TEST(raises_async_sibling_also_has_raises) {
  // The Async sibling repeats the full doc block, and its return wraps the
  // declared throws set as the Future's second parameter.
  BAML_ASSERT(header_text().find(
                  "/// Raises: ParseError, TimeoutError\n"
                  "::baml::future<std::string, "
                  "::baml::variant<::baml_sdk::raises_test::ParseError, "
                  "::baml_sdk::raises_test::TimeoutError>> LoadDoc_async(") !=
              std::string::npos);
}

BAML_TEST(raises_method_raises_blocks) {
  // Methods carry `Raises:` in the header exactly like free functions
  // (python's .pyi-stub analog): both flavors, both variants.
  BAML_ASSERT(header_text().find("  /// Raises: ParseError\n"
                                 "  std::string load(") != std::string::npos);
  BAML_ASSERT(header_text().find("  /// Raises: TimeoutError\n"
                                 "  static ::baml_sdk::raises_test::DocLoader "
                                 "create(") != std::string::npos);
}

BAML_TEST(raises_non_throwing_function_has_no_raises_block) {
  // The summary line abuts the declaration: no Raises line in between.
  BAML_ASSERT(header_text().find("/// A pure function that never throws.\n"
                                 "int64_t PureLen(") != std::string::npos);
}
