package com.boundaryml.baml.gradle;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.Map;
import org.gradle.testkit.runner.BuildResult;
import org.gradle.testkit.runner.GradleRunner;
import org.gradle.testkit.runner.TaskOutcome;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

/**
 * TestKit (GradleRunner) functional tests for the BAML Gradle plugin.
 *
 * <p>Coverage:
 *
 * <ul>
 *   <li>(a) the plugin applies and registers {@code generateBaml} — with no
 *       {@code baml} CLI present (configuration must never fail);
 *   <li>(b) with a fake {@code baml} script on {@code PATH}, a consumer that
 *       references the generated {@code baml_sdk} compiles, and a second build
 *       reports {@code generateBaml} as {@code UP-TO-DATE};
 *   <li>(c) a missing executable fails the task at execution with the install
 *       hint (never at configuration);
 *   <li>(d) the generated source root is backed by the {@code generateBaml} task
 *       provider — the source set carries the task as a build dependency (the
 *       edge IntelliJ reads to generate on sync).
 * </ul>
 */
class BamlPluginFunctionalTest {

    @TempDir Path projectDir;

    // ---- (a) plugin applies + task registered --------------------------------

    @Test
    void pluginAppliesAndRegistersGenerateTask() throws IOException {
        writeSettings("consumer");
        writeFile("build.gradle.kts", """
            plugins {
                id("com.boundaryml.baml")
            }
            """);

        BuildResult result = runner("tasks", "--group", "baml").build();

        assertTrue(
            result.getOutput().contains("generateBaml"),
            () -> "expected generateBaml to be registered; output was:\n" + result.getOutput());
    }

    // ---- (b) end-to-end generate + compile + UP-TO-DATE ----------------------

    @Test
    void generatesCompilesAndIsUpToDateOnRerun() throws IOException {
        writeSettings("consumer");
        writeFile("build.gradle.kts", """
            plugins {
                id("com.boundaryml.baml")
            }
            """);
        writeFile("baml.toml", """
            [package]
            name = "consumer"

            [generator.java_client]
            output_type = "java"
            output_dir = "."
            naming_convention = "preserve-case"
            """);
        writeFile("baml_src/main.baml", """
            function add(a: int, b: int) -> int {
              a + b
            }
            """);
        // Consumer source that references the generated baml_sdk package.
        writeFile("src/main/java/consumer/App.java", """
            package consumer;

            public final class App {
                public static void main(String[] args) {
                    System.out.println(baml_sdk.Baml.hello());
                }
            }
            """);

        Map<String, String> env = envWithFakeBamlOnPath();

        BuildResult first = runner("classes", "--stacktrace")
            .withEnvironment(env)
            .build();

        assertEquals(
            TaskOutcome.SUCCESS,
            first.task(":generateBaml").getOutcome(),
            () -> "generateBaml should run first time; output:\n" + first.getOutput());

        // Generated source landed under build/generated/sources/baml/java/main/baml_sdk.
        assertTrue(
            buildFile("generated/sources/baml/java/main/baml_sdk/Baml.java").exists(),
            "generated Baml.java should exist");
        // The generated + consumer sources both compiled.
        assertTrue(
            buildFile("classes/java/main/baml_sdk/Baml.class").exists(),
            "generated Baml.class should be compiled");
        assertTrue(
            buildFile("classes/java/main/consumer/App.class").exists(),
            "consumer App.class should be compiled against the generated SDK");
        // The bytecode resource rode onto the runtime classpath.
        assertTrue(
            buildFile("resources/main/baml_sdk/inlinedbaml.b64").exists(),
            "inlinedbaml.b64 should be packaged as a resource");

        // Nothing changed → second build is UP-TO-DATE (incremental).
        BuildResult second = runner("classes", "--stacktrace")
            .withEnvironment(env)
            .build();

        assertEquals(
            TaskOutcome.UP_TO_DATE,
            second.task(":generateBaml").getOutcome(),
            () -> "generateBaml should be UP-TO-DATE on rerun; output:\n" + second.getOutput());
    }

    // ---- (c) missing executable → task fails with install hint ----------------

    @Test
    void missingExecutableFailsTaskWithInstallHint() throws IOException {
        writeSettings("consumer");
        writeFile("build.gradle.kts", """
            plugins {
                id("com.boundaryml.baml")
            }

            baml {
                bamlExecutable.set("baml-does-not-exist-xyzzy")
            }
            """);
        writeFile("baml.toml", """
            [package]
            name = "consumer"

            [generator.java_client]
            output_type = "java"
            output_dir = "."
            naming_convention = "preserve-case"
            """);
        writeFile("baml_src/main.baml", """
            function add(a: int, b: int) -> int {
              a + b
            }
            """);

        BuildResult result = runner("generateBaml").buildAndFail();

        assertEquals(
            TaskOutcome.FAILED,
            result.task(":generateBaml").getOutcome(),
            () -> "generateBaml should fail; output:\n" + result.getOutput());
        assertTrue(
            result.getOutput().contains("curl -fsSL https://pkg.boundaryml.com/install.sh"),
            () -> "failure should carry the install hint; output:\n" + result.getOutput());
    }

    // ---- (d) generated source root is wired to the task provider -------------

    /**
     * The generated source root is registered via the {@code generateBaml} task
     * provider ({@code srcDir(generateBaml)}), so Gradle infers the
     * generate→compile dependency from the source set itself — which is what
     * makes IntelliJ generate the sources on Gradle <em>sync</em> (it reads the
     * source-set model, not the task graph of a specific build). Full sync
     * behaviour can't be driven from TestKit, so this asserts the observable
     * model invariant: (1) the generated dir is a Java source root, and (2) the
     * Java source set carries {@code generateBaml} as a build dependency — the
     * edge that a bare-directory {@code srcDir} would NOT create.
     *
     * <p>No {@code baml} CLI and no {@code baml_src} are needed: the wiring
     * exists at configuration time regardless of whether the CLI is present
     * (the task is never executed here).
     */
    @Test
    void generatedSourceRootIsBackedByGenerateBamlTask() throws IOException {
        writeSettings("consumer");
        writeFile("build.gradle.kts", """
            plugins {
                id("com.boundaryml.baml")
            }

            tasks.register("printBamlWiring") {
                val javaSrc = sourceSets["main"].java
                val genDir = layout.buildDirectory
                    .dir("generated/sources/baml/java/main").get().asFile
                doLast {
                    println("BAML_SRC_HAS_GENERATED=" + javaSrc.srcDirs.contains(genDir))
                    val depNames = javaSrc.buildDependencies.getDependencies(null)
                        .map { it.name }.toSortedSet()
                    println("BAML_SRC_TASK_BACKED=" + depNames.contains("generateBaml"))
                    println("BAML_SRC_BUILD_DEPS=" + depNames)
                }
            }
            """);

        BuildResult result = runner("printBamlWiring", "--stacktrace").build();

        assertEquals(
            TaskOutcome.SUCCESS,
            result.task(":printBamlWiring").getOutcome(),
            () -> "printBamlWiring should succeed; output:\n" + result.getOutput());
        assertTrue(
            result.getOutput().contains("BAML_SRC_HAS_GENERATED=true"),
            () -> "the generated dir should be a registered java source root; output:\n"
                + result.getOutput());
        assertTrue(
            result.getOutput().contains("BAML_SRC_TASK_BACKED=true"),
            () -> "the java source set should carry generateBaml as an inferred build "
                + "dependency (srcDir(taskProvider)) so IntelliJ generates on sync; output:\n"
                + result.getOutput());
    }

    // ---- helpers -------------------------------------------------------------

    private GradleRunner runner(String... args) {
        return GradleRunner.create()
            .withProjectDir(projectDir.toFile())
            .withPluginClasspath()
            .withArguments(args);
    }

    private void writeSettings(String rootName) throws IOException {
        writeFile("settings.gradle.kts", "rootProject.name = \"" + rootName + "\"\n");
    }

    private void writeFile(String relativePath, String content) throws IOException {
        Path target = projectDir.resolve(relativePath);
        Files.createDirectories(target.getParent());
        Files.writeString(target, content, StandardCharsets.UTF_8);
    }

    private File buildFile(String relativePath) {
        return projectDir.resolve("build").resolve(relativePath).toFile();
    }

    /**
     * Writes an executable fake {@code baml} script into a fresh {@code bin/}
     * directory and returns an environment map (a copy of the current
     * environment) with that directory prepended to {@code PATH}, so the plugin's
     * default {@code bamlExecutable = "baml"} resolves to the fixture.
     */
    private Map<String, String> envWithFakeBamlOnPath() throws IOException {
        Path binDir = projectDir.resolve("fake-bin");
        Files.createDirectories(binDir);
        Path script = binDir.resolve("baml");
        Files.writeString(script, FAKE_BAML_SCRIPT, StandardCharsets.UTF_8);
        assertTrue(script.toFile().setExecutable(true), "fake baml should be executable");

        Map<String, String> env = new HashMap<>(System.getenv());
        String existingPath = env.getOrDefault("PATH", "");
        env.put("PATH", binDir.toAbsolutePath() + File.pathSeparator + existingPath);
        return env;
    }

    /**
     * A minimal stand-in for the real CLI: answers {@code --version} and, for
     * {@code generate ... -o <dir>}, writes a self-contained {@code baml_sdk}
     * tree (one {@code Baml.java} in package {@code baml_sdk} + an
     * {@code inlinedbaml.b64}) — deliberately free of any {@code baml_bridge}
     * dependency so the compile test stays hermetic.
     */
    private static final String FAKE_BAML_SCRIPT =
        "#!/usr/bin/env bash\n"
        + "set -euo pipefail\n"
        + "\n"
        + "if [ \"${1:-}\" = \"--version\" ]; then\n"
        + "  echo \"baml 0.0.0-testfixture\"\n"
        + "  exit 0\n"
        + "fi\n"
        + "\n"
        + "# Expected: generate --from <dir> -o <outdir> [...]\n"
        + "OUT=\"\"\n"
        + "while [ \"$#\" -gt 0 ]; do\n"
        + "  case \"$1\" in\n"
        + "    -o|--output)\n"
        + "      OUT=\"${2:-}\"\n"
        + "      shift 2\n"
        + "      ;;\n"
        + "    *)\n"
        + "      shift\n"
        + "      ;;\n"
        + "  esac\n"
        + "done\n"
        + "\n"
        + "if [ -z \"$OUT\" ]; then\n"
        + "  echo \"fake baml: no -o argument\" >&2\n"
        + "  exit 1\n"
        + "fi\n"
        + "\n"
        + "mkdir -p \"$OUT\"\n"
        + "\n"
        + "cat > \"$OUT/Baml.java\" <<'JAVA'\n"
        + "package baml_sdk;\n"
        + "\n"
        + "/** Minimal fake of the generated runtime anchor (test fixture). */\n"
        + "public final class Baml {\n"
        + "    private Baml() {}\n"
        + "\n"
        + "    public static String hello() {\n"
        + "        return \"hello from generated baml_sdk\";\n"
        + "    }\n"
        + "}\n"
        + "JAVA\n"
        + "\n"
        + "printf '%s\\n' 'ZmFrZS1iYW1sLWJ5dGVjb2Rl' > \"$OUT/inlinedbaml.b64\"\n";
}
