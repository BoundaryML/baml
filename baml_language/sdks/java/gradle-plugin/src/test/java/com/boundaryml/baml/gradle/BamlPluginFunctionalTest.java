package com.boundaryml.baml.gradle;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;
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
        // manageDependencies = false keeps this compile test hermetic: the fake
        // baml_sdk fixture has no baml_bridge reference, so we don't want the
        // plugin's auto-injected com.boundaryml:baml-bridge dependency (which is
        // not published to any repo this build can see) pulled onto the compile
        // classpath — that resolution would fail with no repositories declared.
        // The injection itself is covered by the dedicated tests below.
        writeFile("build.gradle.kts", """
            plugins {
                id("com.boundaryml.baml")
            }

            baml {
                manageDependencies.set(false)
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

    // ---- (e) dependency management (the one-liner consumer UX) ---------------

    /**
     * Default behaviour: applying the plugin injects the BAML runtime — an
     * {@code implementation} on {@code com.boundaryml:baml-bridge} at the
     * plugin's own version, plus a {@code runtimeOnly} native jar for the build
     * machine's platform. No {@code baml { }} block, no manual dependencies.
     */
    @Test
    void defaultInjectsBridgeAndHostNativeJar() throws IOException {
        String out = runPrintBamlDeps("").getOutput();

        String version = BamlPlugin.pluginVersion();
        String host = BamlPlugin.detectPlatform();

        assertTrue(
            implDeps(out).contains("com.boundaryml:baml-bridge:" + version),
            () -> "expected implementation com.boundaryml:baml-bridge:" + version + "; output:\n" + out);
        assertEquals(
            List.of("natives-" + host),
            nativeClassifiers(out),
            () -> "expected exactly the host native classifier natives-" + host + "; output:\n" + out);
        // A pure-Java consumer must NOT get the Kotlin ergonomics layer.
        assertFalse(
            out.contains("com.boundaryml:baml-bridge-kotlin"),
            () -> "a non-Kotlin consumer should not get baml-bridge-kotlin; output:\n" + out);
    }

    /**
     * A Kotlin consumer (the {@code org.jetbrains.kotlin.jvm} plugin applied)
     * additionally gets {@code com.boundaryml:baml-bridge-kotlin} at the plugin's
     * own version — while {@code baml-bridge} (and its native jar) is still
     * injected. The plugins block declares both the BAML plugin (from the
     * injected TestKit classpath, no version) and Kotlin (from the Plugin
     * Portal); the {@code printBamlDeps} task then reports the declared deps.
     */
    @Test
    void kotlinConsumerAlsoGetsBamlBridgeKotlin() throws IOException {
        writeSettings("consumer");
        writeFile("build.gradle.kts",
            "plugins {\n"
                + "    id(\"com.boundaryml.baml\")\n"
                + "    id(\"org.jetbrains.kotlin.jvm\") version \"1.9.25\"\n"
                + "}\n"
                + "\n"
                + PRINT_BAML_DEPS_TASK);

        String out = runner("printBamlDeps", "--stacktrace").build().getOutput();

        String version = BamlPlugin.pluginVersion();
        assertTrue(
            implDeps(out).contains("com.boundaryml:baml-bridge-kotlin:" + version),
            () -> "a Kotlin consumer should get baml-bridge-kotlin:" + version + "; output:\n" + out);
        // baml-bridge itself is still injected (the native jar rides with it).
        assertTrue(
            implDeps(out).contains("com.boundaryml:baml-bridge:" + version),
            () -> "baml-bridge should still be injected for a Kotlin consumer; output:\n" + out);
    }

    /**
     * A Kotlin consumer that already declares an explicit
     * {@code com.boundaryml:baml-bridge-kotlin} suppresses the Kotlin injection
     * (defer-to-explicit), while an explicit baml-bridge-kotlin does not itself
     * suppress the baml-bridge/native injection.
     */
    @Test
    void explicitBamlBridgeKotlinSuppressesKotlinInjection() throws IOException {
        writeSettings("consumer");
        writeFile("build.gradle.kts",
            "plugins {\n"
                + "    id(\"com.boundaryml.baml\")\n"
                + "    id(\"org.jetbrains.kotlin.jvm\") version \"1.9.25\"\n"
                + "}\n"
                + "\n"
                + "dependencies {\n"
                + "    implementation(\"com.boundaryml:baml-bridge-kotlin:7.7.7-consumer-pin\")\n"
                + "}\n"
                + "\n"
                + PRINT_BAML_DEPS_TASK);

        String out = runner("printBamlDeps", "--stacktrace").build().getOutput();

        // The consumer's own kotlin dep stays; the plugin adds no pinned one.
        assertTrue(
            implDeps(out).contains("com.boundaryml:baml-bridge-kotlin:7.7.7-consumer-pin"),
            () -> "the consumer's explicit baml-bridge-kotlin should remain; output:\n" + out);
        assertFalse(
            implDeps(out).contains("com.boundaryml:baml-bridge-kotlin:" + BamlPlugin.pluginVersion()),
            () -> "the plugin must not inject its own baml-bridge-kotlin when one is explicit; output:\n"
                + out);
        // baml-bridge is still injected (an explicit KOTLIN dep does not suppress it).
        assertTrue(
            implDeps(out).contains("com.boundaryml:baml-bridge:" + BamlPlugin.pluginVersion()),
            () -> "baml-bridge should still be injected; output:\n" + out);
    }

    /**
     * An explicit {@code nativePlatforms} list <em>replaces</em> host detection
     * with exactly the named classifiers (order/dedupe aside), while the
     * {@code baml-bridge} implementation dependency is still injected.
     */
    @Test
    void nativePlatformsOverrideReplacesDetection() throws IOException {
        String out = runPrintBamlDeps("""
            baml {
                nativePlatforms.set(listOf("macos-aarch64", "windows-x86_64"))
            }
            """).getOutput();

        assertTrue(
            implDeps(out).contains("com.boundaryml:baml-bridge:" + BamlPlugin.pluginVersion()),
            () -> "baml-bridge implementation should still be injected; output:\n" + out);
        assertEquals(
            List.of("natives-macos-aarch64", "natives-windows-x86_64"),
            nativeClassifiers(out),
            () -> "explicit nativePlatforms should replace detection; output:\n" + out);
    }

    /**
     * The special value {@code "all"} expands to every classifier in the known
     * set (the six non-experimental targets baml-bridge ships).
     */
    @Test
    void nativePlatformsAllExpandsToEveryKnownPlatform() throws IOException {
        String out = runPrintBamlDeps("""
            baml {
                nativePlatforms.set(listOf("all"))
            }
            """).getOutput();

        List<String> expected = BamlPlugin.ALL_PLATFORMS.stream()
            .map(p -> "natives-" + p)
            .collect(Collectors.toList());
        assertEquals(
            expected,
            nativeClassifiers(out),
            () -> "nativePlatforms=[\"all\"] should add every known platform; output:\n" + out);
    }

    /**
     * {@code manageDependencies = false} is the full escape hatch: the plugin
     * injects nothing at all — no {@code baml-bridge}, no native jar.
     */
    @Test
    void manageDependenciesFalseInjectsNothing() throws IOException {
        String out = runPrintBamlDeps("""
            baml {
                manageDependencies.set(false)
            }
            """).getOutput();

        assertFalse(
            out.contains("com.boundaryml:baml-bridge"),
            () -> "manageDependencies=false should inject no baml-bridge dependency; output:\n" + out);
        assertTrue(
            nativeClassifiers(out).isEmpty(),
            () -> "manageDependencies=false should inject no native jar; output:\n" + out);
    }

    /**
     * A pre-existing explicit {@code com.boundaryml:baml-bridge} dependency (any
     * version, any configuration) suppresses injection entirely: the plugin
     * defers to the consumer's declaration and adds neither its own pinned
     * {@code baml-bridge} nor a native jar.
     */
    @Test
    void preExistingBridgeDependencySuppressesInjection() throws IOException {
        String out = runPrintBamlDeps("""
            dependencies {
                implementation("com.boundaryml:baml-bridge:9.9.9-consumer-pin")
            }
            """).getOutput();

        // The consumer's own dependency is present...
        assertTrue(
            implDeps(out).contains("com.boundaryml:baml-bridge:9.9.9-consumer-pin"),
            () -> "the consumer's explicit baml-bridge should remain; output:\n" + out);
        // ...and the plugin did NOT add its own pinned version.
        assertFalse(
            implDeps(out).contains("com.boundaryml:baml-bridge:" + BamlPlugin.pluginVersion()),
            () -> "the plugin must not inject its own baml-bridge when one is explicit; output:\n" + out);
        // ...and no native jar was injected either.
        assertTrue(
            nativeClassifiers(out).isEmpty(),
            () -> "no native jar should be injected when baml-bridge is explicit; output:\n" + out);
    }

    // ---- helpers -------------------------------------------------------------

    /**
     * Applies the plugin, appends {@code extraBuildScript} (a {@code baml { }}
     * and/or {@code dependencies { }} block), registers a {@code printBamlDeps}
     * task that prints the <em>declared</em> {@code implementation} /
     * {@code runtimeOnly} dependencies (never resolving them — the injected
     * baml-bridge is not published to any reachable repo), and runs it.
     */
    private BuildResult runPrintBamlDeps(String extraBuildScript) throws IOException {
        writeSettings("consumer");
        writeFile("build.gradle.kts",
            "plugins {\n"
                + "    id(\"com.boundaryml.baml\")\n"
                + "}\n"
                + "\n"
                + extraBuildScript
                + "\n"
                + PRINT_BAML_DEPS_TASK);
        return runner("printBamlDeps", "--stacktrace").build();
    }

    /** {@code group:name:version} strings from the {@code IMPL=} lines. */
    private static List<String> implDeps(String output) {
        return output.lines()
            .filter(l -> l.startsWith("IMPL="))
            .map(l -> l.substring("IMPL=".length()))
            .collect(Collectors.toList());
    }

    /**
     * The native-jar classifiers from the {@code RUNTIME=} lines (each line is
     * {@code RUNTIME=group:name:version|classifier}), in declaration order,
     * skipping any runtimeOnly dep without a classifier.
     */
    private static List<String> nativeClassifiers(String output) {
        return output.lines()
            .filter(l -> l.startsWith("RUNTIME="))
            .map(l -> l.substring(l.indexOf('|') + 1))
            .filter(c -> !c.isEmpty())
            .collect(Collectors.toList());
    }

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
        env.put("EXPECTED_BAML_PROJECT", projectDir.toFile().getCanonicalPath());
        return env;
    }

    /**
     * A minimal stand-in for the real CLI: answers {@code --version} and, for
     * {@code bridge generate ... -o <dir>}, writes a self-contained {@code baml_sdk}
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
        + "# Expected: bridge generate --project <dir> -o <outdir> [...]\n"
        + "if [ \"$#\" -ne 6 ] || [ \"${1:-}\" != \"bridge\" ]"
        + " || [ \"${2:-}\" != \"generate\" ] || [ \"${3:-}\" != \"--project\" ]"
        + " || [ \"${4:-}\" != \"${EXPECTED_BAML_PROJECT:-}\" ]"
        + " || [ \"${5:-}\" != \"-o\" ]; then\n"
        + "  echo \"fake baml: unexpected arguments: $*\" >&2\n"
        + "  exit 1\n"
        + "fi\n"
        + "\n"
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

    /**
     * A consumer task that prints the <em>declared</em> dependencies of the
     * {@code implementation} and {@code runtimeOnly} configurations — used by the
     * dependency-management tests to observe what the plugin injected without
     * resolving anything (the injected com.boundaryml:baml-bridge is not on any
     * reachable repository). Native jars carry their classifier after a {@code |}.
     */
    private static final String PRINT_BAML_DEPS_TASK =
        "tasks.register(\"printBamlDeps\") {\n"
        + "    doLast {\n"
        + "        configurations.getByName(\"implementation\").dependencies.forEach { d ->\n"
        + "            println(\"IMPL=\" + d.group + \":\" + d.name + \":\" + d.version)\n"
        + "        }\n"
        + "        configurations.getByName(\"runtimeOnly\").dependencies.forEach { d ->\n"
        + "            val classifier = (d as? org.gradle.api.artifacts.ModuleDependency)\n"
        + "                ?.artifacts?.joinToString(\",\") { it.classifier ?: \"\" } ?: \"\"\n"
        + "            println(\"RUNTIME=\" + d.group + \":\" + d.name + \":\" + d.version + \"|\" + classifier)\n"
        + "        }\n"
        + "    }\n"
        + "}\n";
}
