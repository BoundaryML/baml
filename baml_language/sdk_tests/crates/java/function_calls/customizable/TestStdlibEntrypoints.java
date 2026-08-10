// Stdlib functions of different `FunctionKind`s are callable as entry points
// (directly from the host), not only from inside BAML.
//
// Port of python_pydantic2/function_calls/customizable/test_stdlib_entrypoints.py.
//
// java-port note: BAML `baml.sys` / `baml.fs` namespaces map to packages
// `baml_sdk.baml.sys` / `baml_sdk.baml.fs`, each with its own `Fns` holder.
// Two `Fns` classes can't both be imported (no import aliases in Java), so
// they are fully qualified at the call site.

import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import org.junit.jupiter.api.Test;

class TestStdlibEntrypoints {

    // Intrinsic-only modules are not emitted at all, so a missing file is fine;
    // callers only need to confirm the symbol is absent when the file exists.
    private static String generatedSdkFile(String relPath) throws IOException {
        Path path = Paths.get("baml_sdk", relPath);
        if (!Files.exists(path)) {
            return null;
        }
        return Files.readString(path);
    }

    // `baml.sys.argv() -> string[]` is a `$rust_function` ->
    // `FunctionKind::Native`. Calling it as an entry point should run the
    // native and return the argument list, not reject with
    // `NotInvokableAsEntry`. The fixture host passes no program arguments, so
    // the contents are not worth asserting on — that the call lands and
    // returns a list is.
    @Test
    void test_stdlib_entrypoints_native_argv_callable_as_entry_point() {
        assertNotNull(baml_sdk.baml.sys.Fns.argv());
    }

    // `baml.fs.exists(path: string) -> bool` is a `$rust_io_function` ->
    // `FunctionKind::SysOp`. Calling it as an entry point should run the
    // filesystem sysop and return a bool. `.` exists in the generated fixture
    // directory on the test host.
    @Test
    void test_stdlib_entrypoints_sysop_fs_exists_callable_as_entry_point() {
        assertTrue(baml_sdk.baml.fs.Fns.exists("."));
    }

    @Test
    void test_stdlib_entrypoints_compiler_intrinsics_are_not_emitted_as_entry_points() throws IOException {
        // java-port note: Python inspects both the `.py` (wire FQN string
        // literals) and the `.pyi` (stub `def` decls). Java emits a single
        // `Fns.java` per namespace, so each row checks that holder for either
        // the wire FQN literal or the generated method declaration.
        String[][] forbidden = {
            {"vendor/log/Fns.java", "\"log.info\""},
            {"vendor/log/Fns.java", "\"log.debug\""},
            {"vendor/log/Fns.java", "\"log.warn\""},
            {"vendor/log/Fns.java", "\"log.error\""},
            {"vendor/log/Fns.java", "info("},
            {"vendor/log/Fns.java", "debug("},
            {"vendor/log/Fns.java", "warn("},
            {"vendor/log/Fns.java", "error("},
            {"baml/events/Fns.java", "\"baml.events.send\""},
            {"baml/events/Fns.java", "send("},
        };

        for (String[] entry : forbidden) {
            String contents = generatedSdkFile(entry[0]);
            assertTrue(contents == null || !contents.contains(entry[1]));
        }
    }
}
