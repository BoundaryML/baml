// BAML `///` doc-comment lowering coverage.
//
// Port of python_pydantic2/docstrings_etc/customizable/test_main.py —
// same test names, cases, assertions.
//
// ===========================================================================
// java-port note: runtime `__doc__` vs. Javadoc
// ---------------------------------------------------------------------------
// The Python original asserts on the *runtime* `__doc__` of generated classes
// and enums (via `inspect.getdoc(...)`). The JVM has no runtime analog of a
// Python docstring — Javadoc is a source-only comment, discarded by `javac`
// and absent from the loaded `Class`. So there is nothing to read off a live
// `Class<?>` at test time.
//
// Closest meaningful port: read the *generated `.java` source* and assert the
// Javadoc content. The Gradle `test` task runs with its working directory at
// the fixture's `generated/` root (the codegen output lives at
// `generated/baml_sdk/<ns>/<Class>.java` and the JUnit sources at
// `generated/tests/`), so a path relative to the working dir —
// `baml_sdk/docs/Doc.java` — resolves to the generated source. This mirrors the
// Python test's own `test_no_inline_field_or_variant_doc_artifacts`, which
// already reaches for `mod.__file__` and greps the generated source.
//
// The emission contract these tests pin (INVENTED — needs human review; the
// codegen-conventions doc does not yet specify a Javadoc format):
//   * a class/enum `///` lowers to a single `/** ... */` Javadoc block
//     immediately preceding the declaration, rendered as plain text (no HTML
//     `<ul>`/`<li>`), with the same "Attributes:" / "Members:" sections and
//     `name: doc` / bare-`name` lines the Python emitter produces;
//   * a field/variant `///` never produces an inline `//` comment or a
//     per-member Javadoc block — it lives only inside the parent's section.
// `javadocOf(...)` below reconstructs the block's text by stripping the ` * `
// decoration, the Java analog of `inspect.getdoc` (dedent + trim), so the
// assertions pin doc *content* and tolerate Javadoc decoration.
// ===========================================================================

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_sdk.docs.Doc;
import baml_sdk.docs.Note;
import baml_sdk.docs.Priority;
import baml_sdk.docs.Sentiment;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Set;
import java.util.stream.Collectors;
import org.junit.jupiter.api.Test;

class TestMain {

    // --- helpers -----------------------------------------------------------

    /** Read a generated `baml_sdk/docs/<file>` source, relative to the Gradle
     *  test working dir (`generated/`). Java analog of Python's `mod.__file__`
     *  + `open(...)`. */
    private static String readSource(String file) throws IOException {
        return Files.readString(Path.of("baml_sdk", "docs", file));
    }

    /** Reconstruct the text of the Javadoc block immediately preceding the
     *  declaration whose line contains `declSubstring`. Strips the ` * `
     *  decoration and trims leading/trailing blank lines — the Java analog of
     *  `inspect.getdoc(...)`. */
    private static String javadocOf(String source, String declSubstring) {
        int declIdx = source.indexOf(declSubstring);
        assertTrue(declIdx >= 0, "declaration not found: " + declSubstring);
        int open = source.lastIndexOf("/**", declIdx);
        assertTrue(open >= 0, "no Javadoc block precedes: " + declSubstring);
        int close = source.indexOf("*/", open);
        assertTrue(close > open, "unterminated Javadoc block before: " + declSubstring);

        String block = source.substring(open + 3, close);
        List<String> lines = new ArrayList<>();
        for (String raw : block.split("\n", -1)) {
            String s = raw.replaceFirst("^\\s*\\*", "");
            if (s.startsWith(" ")) {
                s = s.substring(1);
            }
            s = s.replaceAll("\\s+$", "");
            lines.add(s);
        }
        // Drop leading/trailing empty lines (inspect.getdoc semantics).
        int start = 0;
        int end = lines.size();
        while (start < end && lines.get(start).isEmpty()) {
            start++;
        }
        while (end > start && lines.get(end - 1).isEmpty()) {
            end--;
        }
        return String.join("\n", lines.subList(start, end));
    }

    private static int countJavadocBlocks(String source) {
        int count = 0;
        int idx = source.indexOf("/**");
        while (idx >= 0) {
            count++;
            idx = source.indexOf("/**", idx + 3);
        }
        return count;
    }

    // --- tests -------------------------------------------------------------

    @Test
    void test_main_imports_symbols_reachable() {
        // java-port note: the Python `import baml_sdk[.docs]` smoke maps to
        // referencing each generated symbol's `.class` (compile-time
        // reachability + class-load side effects), per the codegen-conventions
        // doc's namespace-import rule.
        assertNotNull(Doc.class);
        assertNotNull(Note.class);
        assertNotNull(Priority.class);
        assertNotNull(Sentiment.class);
    }

    @Test
    void test_main_class_doc_summary_and_attributes_section() throws IOException {
        String src = readSource("Doc.java");
        String doc = javadocOf(src, "class Doc");
        String expected =
                "A document with a title and an optional body.\n"
                        + "\n"
                        + "Attributes:\n"
                        + "    title: Title shown in lists and search results.\n"
                        + "    body: Free-form body text.";
        assertEquals(expected, doc, "got:\n" + doc);
    }

    @Test
    void test_main_undocumented_field_listed_as_bare_name_under_attributes() throws IOException {
        // Note: `id` is documented, `text` is not. The "any-doc" rule says the
        // Attributes: section appears (because `id` carries a `///`) and lists
        // every field, with `text` rendered as a bare name.
        String src = readSource("Note.java");
        String doc = javadocOf(src, "class Note");
        assertTrue(doc.startsWith("A multi-line summary.\nContinuation line"), "got:\n" + doc);
        assertTrue(doc.contains("\n\nAttributes:\n    id: Stable identifier"), "got:\n" + doc);
        // Bare-name entry: just the identifier, no trailing colon, no doc.
        assertTrue(doc.stripTrailing().endsWith("\n    text"), "got:\n" + doc);
    }

    @Test
    void test_main_enum_doc_summary_and_members_section() throws IOException {
        String src = readSource("Sentiment.java");
        String doc = javadocOf(src, "enum Sentiment");
        String expected =
                "Sentiment labels surfaced by the model.\n"
                        + "\n"
                        + "Members:\n"
                        + "    HAPPY: Smiling face.\n"
                        + "    SAD: Frowning face.\n"
                        + "    NEUTRAL";
        assertEquals(expected, doc, "got:\n" + doc);
    }

    @Test
    void test_main_enum_summary_only_omits_members_section()
            throws IOException {
        // Priority has a class-level /// but no variant carries one — the
        // Members: section should be suppressed entirely. Variants are still
        // importable / iterable normally.
        String src = readSource("Priority.java");
        String doc = javadocOf(src, "enum Priority");
        assertEquals(
                "Pin the \"summary only, no member rollup\" case: this enum has a\n"
                        + "class-level `///` but every variant is bare.",
                doc,
                "got:\n" + doc);
        assertFalse(doc.contains("Members:"), "Members: section leaked in:\n" + doc);
        // java-port note: Python asserts on str-enum `.value`; the Java analog
        // of the variant's wire spelling is the (PreserveCase) constant name.
        Set<String> values =
                Arrays.stream(Priority.values()).map(Enum::name).collect(Collectors.toSet());
        assertEquals(Set.of("HIGH", "MEDIUM", "LOW"), values);
    }

    @Test
    void test_main_no_inline_field_or_variant_doc_artifacts() throws IOException {
        // Field/variant `///` lines must not produce inline `// …` comments or
        // per-member Javadoc — they live exclusively inside the parent's
        // Attributes:/Members: section.
        //
        // java-port note: Python reads the single `baml_sdk/docs/__init__.py`
        // module file and counts `"""` fences (4 docstrings × 2 = 8). Java
        // emits one public class per file, so the analog is: each of the four
        // parents' source files carries exactly one `/** */` block (its own
        // class/enum doc) and no field/variant doc leaks as a `//` line.
        for (String file : List.of("Doc.java", "Note.java", "Sentiment.java", "Priority.java")) {
            String src = readSource(file);
            assertFalse(src.contains("// Title shown in lists"), src);
            assertFalse(src.contains("// Smiling face"), src);
            assertEquals(1, countJavadocBlocks(src), "unexpected Javadoc-block count in " + file);
        }
    }
}
