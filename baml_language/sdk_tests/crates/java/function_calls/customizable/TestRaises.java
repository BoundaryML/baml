// Throws-contract documentation coverage (32d analog).
//
// Port of python_pydantic2/function_calls/customizable/test_raises.py.
//
// java-port note: the Python original pins how the generator renders the
// inferred throws contract (`callable_throws`) into a Google-style `Raises:`
// docstring block, read at runtime via `inspect.getdoc` (free functions) or by
// reading the `.pyi` stub source (methods). Java has no runtime docstrings, and
// the completeness doc commits the throws contract to Javadoc `@throws` tags
// (ref-java-state-of-completeness.md: "thrown types documented in Javadoc
// `@throws`"). So this suite reads the GENERATED `.java` source and asserts on
// the `@throws <UnqualifiedName>` tags, mirroring the Python test that already
// reads the `.pyi` source (`test_method_raises_block_in_pyi`). Unlike Python,
// Java has no separate stub, so both free functions and methods carry their
// `@throws` in the same generated source.
//
// The source is read from `baml_sdk/raises_test/*.java` relative to the test
// working directory, the same convention as TestStdlibEntrypoints.

import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_sdk.raises_test.DocLoader;
import baml_sdk.raises_test.Fns;
import baml_sdk.raises_test.ParseError;
import baml_sdk.raises_test.TimeoutError;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import org.junit.jupiter.api.Test;

class TestRaises {

    private static String source(String simpleName) throws IOException {
        Path path = Paths.get("baml_sdk", "raises_test", simpleName + ".java");
        return Files.readString(path);
    }

    /** The Javadoc block immediately preceding the method whose signature
     * contains `methodMarker` (e.g. "LoadDoc(" or "load("), up to the
     * signature. The Java analog of test_raises.py's `_def_block`. */
    private static String javadocFor(String src, String methodMarker) {
        int mIdx = src.indexOf(methodMarker);
        if (mIdx < 0) {
            return "";
        }
        int docStart = src.lastIndexOf("/**", mIdx);
        if (docStart < 0) {
            return "";
        }
        int docEnd = src.indexOf("*/", docStart);
        if (docEnd < 0 || docEnd > mIdx) {
            return "";
        }
        return src.substring(docStart, mIdx);
    }

    @Test
    void test_raises_imports_symbols_reachable() {
        // Namespace-import smoke test: reference known generated symbols so the
        // packages are compile-time reachable. The free functions LoadDoc /
        // Reparse / InferredThrow / PureLen are static methods on `Fns`; the
        // error types and DocLoader are classes.
        assertNotNull(Fns.class);
        assertNotNull(DocLoader.class);
        assertNotNull(ParseError.class);
        assertNotNull(TimeoutError.class);
    }

    @Test
    void test_raises_union_throws_lists_all_names() throws IOException {
        String block = javadocFor(source("Fns"), "LoadDoc(");
        assertTrue(block.contains("@throws ParseError"), block);
        assertTrue(block.contains("@throws TimeoutError"), block);
    }

    @Test
    void test_raises_async_sibling_also_has_raises() throws IOException {
        String block = javadocFor(source("Fns"), "LoadDoc_async(");
        assertTrue(block.contains("@throws ParseError"), block);
        assertTrue(block.contains("@throws TimeoutError"), block);
    }

    @Test
    void test_raises_single_throws() throws IOException {
        String block = javadocFor(source("Fns"), "Reparse(");
        assertTrue(block.contains("@throws ParseError"), block);
    }

    @Test
    void test_raises_summary_precedes_raises_block() throws IOException {
        String block = javadocFor(source("Fns"), "LoadDoc(");
        int summaryAt = block.indexOf("Load a document from a path.");
        int throwsAt = block.indexOf("@throws");
        assertTrue(summaryAt >= 0, block);
        assertTrue(throwsAt >= 0, block);
        assertTrue(summaryAt < throwsAt, block);
    }

    @Test
    void test_raises_inferred_contract_without_clause_still_raises() throws IOException {
        // No written `throws` clause, but the body throws ParseError — the
        // inferred contract (callable_throws) still surfaces a `@throws` tag.
        String block = javadocFor(source("Fns"), "InferredThrow(");
        assertTrue(block.contains("@throws ParseError"), block);
    }

    @Test
    void test_raises_non_throwing_function_has_no_raises_block() throws IOException {
        String block = javadocFor(source("Fns"), "PureLen(");
        assertTrue(!block.contains("@throws"), block);
    }

    @Test
    void test_raises_method_raises_block_in_pyi() throws IOException {
        // java-port note: Python asserts methods carry `Raises:` in the `.pyi`
        // stub (the 32d decision) because their runtime `.py` __doc__ is
        // free-functions-only. Java has no split stub — methods carry `@throws`
        // in the same generated `DocLoader.java`.
        String src = source("DocLoader");

        String loadBlock = javadocFor(src, "load(");
        assertTrue(loadBlock.contains("@throws ParseError"), loadBlock);

        String createBlock = javadocFor(src, "create(");
        assertTrue(createBlock.contains("@throws TimeoutError"), createBlock);
    }
}
