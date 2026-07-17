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

static std::string HeaderText() {
  std::ifstream in("baml_sdk/include/baml_sdk.h");
  if (!in) {
    baml_test::Fail("cannot open baml_sdk/include/baml_sdk.h");
  }
  std::ostringstream out;
  out << in.rdbuf();
  return out.str();
}

BAML_TEST(union_throws_lists_all_names) {
  // A multi-member throws union lists every member, unqualified.
  BAML_ASSERT(HeaderText().find("/// Raises: ParseError, TimeoutError\n"
                                "std::string LoadDoc(") != std::string::npos);
}

BAML_TEST(single_throws) {
  BAML_ASSERT(HeaderText().find("/// Raises: ParseError\n"
                                "std::string Reparse(") != std::string::npos);
}

BAML_TEST(summary_precedes_raises_block) {
  BAML_ASSERT(HeaderText().find("/// Load a document from a path.\n"
                                "/// Raises: ParseError, TimeoutError\n"
                                "std::string LoadDoc(") != std::string::npos);
}

BAML_TEST(inferred_contract_without_clause_still_raises) {
  // No written `throws` clause, but the body throws ParseError -- the
  // inferred contract (callable_throws) still surfaces a Raises line.
  BAML_ASSERT(HeaderText().find("/// Raises: ParseError\n"
                                "std::string InferredThrow(") !=
              std::string::npos);
}

BAML_TEST(async_sibling_also_has_raises) {
  // The Async sibling repeats the full doc block, and its return wraps the
  // declared throws set as the Future's second parameter.
  BAML_ASSERT(HeaderText().find(
                  "/// Raises: ParseError, TimeoutError\n"
                  "::baml::Future<std::string, "
                  "::baml::Union<::baml_sdk::raises_test::ParseError, "
                  "::baml_sdk::raises_test::TimeoutError>> LoadDocAsync(") !=
              std::string::npos);
}

BAML_TEST(non_throwing_function_has_no_raises_block) {
  // The summary line abuts the declaration: no Raises line in between.
  BAML_ASSERT(HeaderText().find("/// A pure function that never throws.\n"
                                "int64_t PureLen(") != std::string::npos);
}
