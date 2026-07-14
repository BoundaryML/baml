// BAML /// doc-comment lowering coverage. Port of
// docstrings_etc/customizable/test_main.py: Python asserts on runtime
// __doc__; the C++ surface is the generated header's /// doc comments with
// the same Attributes:/Members: rollup rules.
#include <fstream>
#include <sstream>
#include <string>

#include <baml_sdk.hpp>
#include <baml_test.hpp>

using baml_sdk::docs::Doc;
using baml_sdk::docs::Note;
using baml_sdk::docs::Priority;
using baml_sdk::docs::Sentiment;

static std::string header_text() {
    std::ifstream in("baml_sdk/include/baml_sdk.hpp");
    if (!in) {
        baml_test::fail("cannot open baml_sdk/include/baml_sdk.hpp");
    }
    std::ostringstream out;
    out << in.rdbuf();
    return out.str();
}

static size_t count_occurrences(const std::string& text, const std::string& needle) {
    size_t count = 0;
    for (size_t pos = text.find(needle); pos != std::string::npos;
         pos = text.find(needle, pos + needle.size())) {
        ++count;
    }
    return count;
}

BAML_TEST(class_doc_summary_and_attributes_section) {
    const std::string expected =
        "/// A document with a title and an optional body.\n"
        "///\n"
        "/// Attributes:\n"
        "///     title: Title shown in lists and search results.\n"
        "///     body: Free-form body text.\n"
        "struct Doc {";
    BAML_ASSERT(header_text().find(expected) != std::string::npos);
}

BAML_TEST(undocumented_field_listed_as_bare_name_under_attributes) {
    const std::string text = header_text();
    // Note: `id` is documented, `text` is not - the any-doc rule lists every
    // field, the undocumented one as a bare name.
    BAML_ASSERT(text.find("/// A multi-line summary.\n/// Continuation line") !=
                std::string::npos);
    BAML_ASSERT(text.find("/// Attributes:\n///     id: Stable identifier") != std::string::npos);
    BAML_ASSERT(text.find("///     text\nstruct Note {") != std::string::npos);
}

BAML_TEST(enum_doc_summary_and_members_section) {
    const std::string expected =
        "/// Sentiment labels surfaced by the model.\n"
        "///\n"
        "/// Members:\n"
        "///     HAPPY: Smiling face.\n"
        "///     SAD: Frowning face.\n"
        "///     NEUTRAL\n"
        "enum class Sentiment {";
    BAML_ASSERT(header_text().find(expected) != std::string::npos);
}

BAML_TEST(enum_summary_only_omits_members_section) {
    const std::string text = header_text();
    const size_t decl = text.find("enum class Priority {");
    BAML_ASSERT(decl != std::string::npos);
    const std::string before = text.substr(decl < 400 ? 0 : decl - 400,
                                           decl < 400 ? decl : size_t{400});
    BAML_ASSERT(before.find("Members:") == std::string::npos);
    // Variants still usable.
    BAML_ASSERT(Priority::HIGH != Priority::LOW);
    (void)Priority::MEDIUM;
}

BAML_TEST(no_inline_field_or_variant_doc_artifacts) {
    // Field/variant /// docs live exclusively in the parent's rollup, never
    // inline per field. The class doc appears twice - once on the class and
    // once on its stream_types:: companion, both rollups; the enum has no
    // companion, so its variant doc appears exactly once.
    const std::string text = header_text();
    BAML_ASSERT_EQ(count_occurrences(text, "Title shown in lists"), size_t{2});
    BAML_ASSERT_EQ(count_occurrences(text, "Smiling face."), size_t{1});
}

BAML_TEST_MAIN()
