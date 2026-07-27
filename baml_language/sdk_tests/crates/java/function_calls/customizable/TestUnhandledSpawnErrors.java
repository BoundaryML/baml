import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.BamlFfi;
import baml_sdk.Fns;
import java.nio.charset.StandardCharsets;
import org.junit.jupiter.api.Test;

class TestUnhandledSpawnErrors {
    @Test
    void test_unhandled_spawn_error_uses_host_default() throws Exception {
        String javaBin = System.getProperty("java.home") + "/bin/java";
        String classpath = System.getProperty("java.class.path");
        Process process =
                new ProcessBuilder(
                                javaBin,
                                "-cp",
                                classpath,
                                "TestUnhandledSpawnErrors$UnhandledSpawnSnippet")
                        .redirectErrorStream(true)
                        .start();

        String output = new String(process.getInputStream().readAllBytes(), StandardCharsets.UTF_8);
        assertEquals(0, process.waitFor(), output);
        assertTrue(output.contains("user.unhandled_spawn_error"), output);
    }

    public static final class UnhandledSpawnSnippet {
        public static void main(String[] args) {
            assertEquals(1L, Fns.spawn_unhandled_error());
            BamlFfi.shutdownRuntime();
        }
    }
}
