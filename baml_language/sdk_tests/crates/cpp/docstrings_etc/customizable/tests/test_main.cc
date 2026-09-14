// BAML /// doc-comment lowering coverage. Port of
// docstrings_etc/customizable/test_main.py: Python asserts on runtime
// __doc__; the C++ surface is the generated header's /// doc comments with
// the same Attributes:/Members: rollup rules.
#include <baml_sdk.h>
#include <baml_test.h>

#include <fstream>
#include <sstream>
#include <string>

using baml_sdk::docs::Priority;

static std::string header_text() {
  std::ifstream in("baml_sdk/include/baml_sdk.h");
  if (!in) {
    baml_test::fail("cannot open baml_sdk/include/baml_sdk.h");
  }
  std::ostringstream out;
  out << in.rdbuf();
  return out.str();
}

static size_t CountOccurrences(const std::string& text,
                               const std::string& needle) {
  size_t count = 0;
  for (size_t pos = text.find(needle); pos != std::string::npos;
       pos = text.find(needle, pos + needle.size())) {
    ++count;
  }
  return count;
}

BAML_TEST(main_class_doc_summary_and_attributes_section) {
  // Anchored on the namespace open so nothing precedes the summary line
  // (python parity: exact __doc__ equality).
  const std::string expected =
      "namespace docs {\n"
      "/// A document with a title and an optional body.\n"
      "///\n"
      "/// Attributes:\n"
      "///     title: Title shown in lists and search results.\n"
      "///     body: Free-form body text.\n"
      "struct Doc {";
  BAML_ASSERT(header_text().find(expected) != std::string::npos);
}

BAML_TEST(main_undocumented_field_listed_as_bare_name_under_attributes) {
  // Note: `id` is documented, `text` is not - the any-doc rule lists every
  // field, the undocumented one as a bare name. One contiguous block
  // (python parity: exact __doc__ equality), pinning the blank separator
  // line between the summary and the Attributes: section.
  const std::string expected =
      "namespace docs {\n"
      "/// A multi-line summary.\n"
      "/// Continuation line of the summary, preserved verbatim in the\n"
      "/// rendered block-form docstring.\n"
      "///\n"
      "/// Attributes:\n"
      "///     id: Stable identifier \u2014 surfaces in URLs.\n"
      "///     text\n"
      "struct Note {";
  BAML_ASSERT(header_text().find(expected) != std::string::npos);
}

BAML_TEST(main_enum_doc_summary_and_members_section) {
  const std::string expected =
      "namespace docs {\n"
      "/// Sentiment labels surfaced by the model.\n"
      "///\n"
      "/// Members:\n"
      "///     HAPPY: Smiling face.\n"
      "///     SAD: Frowning face.\n"
      "///     NEUTRAL\n"
      "// Enumerator values: FNV-1a-64 of the wire value (reorder-stable).\n"
      "enum class Sentiment : uint64_t {";
  BAML_ASSERT(header_text().find(expected) != std::string::npos);
}

BAML_TEST(main_enum_summary_only_omits_members_section) {
  const std::string text = header_text();
  // The class-level summary is present verbatim with NO Members: rollup
  // between it and the declaration (python parity: exact __doc__
  // equality for the summary-only case).
  const std::string expected =
      "namespace docs {\n"
      "/// Pin the \"summary only, no member rollup\" case: this enum has a\n"
      "/// class-level `///` but every variant is bare.\n"
      "// Enumerator values: FNV-1a-64 of the wire value (reorder-stable).\n"
      "enum class Priority : uint64_t {";
  const size_t decl = text.find(expected);
  BAML_ASSERT(decl != std::string::npos);
  // Exactly three members (python parity: {m.value} set equality pins
  // exhaustiveness)...
  const size_t body_start = decl + expected.size();
  const size_t body_end = text.find("};", body_start);
  BAML_ASSERT(body_end != std::string::npos);
  const std::string body = text.substr(body_start, body_end - body_start);
  BAML_ASSERT_EQ(CountOccurrences(body, "="), size_t{3});
  // ...with these wire values.
  BAML_ASSERT_EQ(std::string(baml::codec<Priority>::ToWire(Priority::HIGH)),
                 std::string("HIGH"));
  BAML_ASSERT_EQ(std::string(baml::codec<Priority>::ToWire(Priority::MEDIUM)),
                 std::string("MEDIUM"));
  BAML_ASSERT_EQ(std::string(baml::codec<Priority>::ToWire(Priority::LOW)),
                 std::string("LOW"));
  BAML_ASSERT(Priority::HIGH != Priority::LOW);
}

BAML_TEST(main_no_inline_field_or_variant_doc_artifacts) {
  // Field/variant /// docs live exclusively in each parent's rollup, never
  // inline per field. Classes have an authored and a generated stream-partial
  // parent; enums do not need a stream-partial shadow.
  const std::string text = header_text();
  BAML_ASSERT_EQ(CountOccurrences(text, "Title shown in lists"), size_t{2});
  BAML_ASSERT_EQ(CountOccurrences(text, "Smiling face."), size_t{1});
}

BAML_TEST_MAIN()
