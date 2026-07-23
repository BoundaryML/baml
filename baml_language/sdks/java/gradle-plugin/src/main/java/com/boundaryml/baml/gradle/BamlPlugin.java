package com.boundaryml.baml.gradle;

import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Properties;
import java.util.Set;
import java.util.concurrent.TimeUnit;
import org.gradle.api.Plugin;
import org.gradle.api.Project;
import org.gradle.api.artifacts.Configuration;
import org.gradle.api.artifacts.Dependency;
import org.gradle.api.artifacts.dsl.DependencyHandler;
import org.gradle.api.file.Directory;
import org.gradle.api.plugins.JavaPlugin;
import org.gradle.api.plugins.JavaPluginExtension;
import org.gradle.api.provider.Provider;
import org.gradle.api.provider.ProviderFactory;
import org.gradle.api.tasks.SourceSet;
import org.gradle.api.tasks.TaskProvider;
import org.gradle.language.jvm.tasks.ProcessResources;

/**
 * The BAML Gradle plugin ({@code com.boundaryml.baml}) — pattern C of
 * ref-java-packaging.md: generate the typed BAML Java SDK at build time.
 *
 * <p>Applying the plugin:
 *
 * <ul>
 *   <li>applies the {@code java} plugin (so the source sets exist);
 *   <li>registers the {@code baml { ... }} extension;
 *   <li>registers the cacheable {@link GenerateBamlTask} {@code generateBaml};
 *   <li>adds {@code generateBaml}'s output as a Java source root and its
 *       {@code baml_sdk/**&#47;*.b64} bytecode as a resource, and makes
 *       {@code compileJava} / {@code processResources} depend on it;
 *   <li>auto-manages the BAML runtime dependencies (the JavaFX-plugin pattern):
 *       injects {@code com.boundaryml:baml-bridge} at the plugin's own version
 *       plus the native jar for the build machine, so
 *       {@code plugins { id("com.boundaryml.baml") version "X" }} is the entire
 *       setup. See {@link BamlExtension} for the opt-outs.
 * </ul>
 *
 * <p>The generated tree lands under
 * {@code build/generated/sources/baml/java/main} and is (re)generated only when
 * {@code baml.toml}, {@code baml_src/**}, or the resolved CLI version change.
 */
public class BamlPlugin implements Plugin<Project> {

    private static final String EXTENSION_NAME = "baml";
    private static final String TASK_NAME = "generateBaml";
    private static final String OUTPUT_DIR = "generated/sources/baml/java/main";

    /** The BAML JVM runtime coordinates (the artifact the generated SDK compiles/links against). */
    static final String BRIDGE_GROUP = "com.boundaryml";
    static final String BRIDGE_ARTIFACT = "baml-bridge";

    /**
     * The Kotlin ergonomics layer. Injected (at the plugin's own version) ONLY
     * when the consumer applies the Kotlin JVM plugin, so a pure-Java consumer
     * never pulls it. It brings baml-bridge transitively (its POM declares it as
     * an api dependency), but baml-bridge is injected directly regardless (the
     * native jar rides with that).
     */
    static final String KOTLIN_ARTIFACT = "baml-bridge-kotlin";

    /** The Kotlin JVM plugin id whose presence gates the {@link #KOTLIN_ARTIFACT} injection. */
    static final String KOTLIN_JVM_PLUGIN_ID = "org.jetbrains.kotlin.jvm";

    /**
     * The full known set of native-jar platform classifiers, mirroring the
     * targets {@code baml_bridge} publishes (release/platforms.json → the six
     * non-experimental {@code java} targets). This is what the extension's
     * {@code nativePlatforms = ["all"]} expands to. Musl classifiers are
     * deliberately excluded (experimental, explicit-request only).
     */
    static final List<String> ALL_PLATFORMS = List.of(
        "linux-x86_64",
        "linux-aarch64",
        "macos-x86_64",
        "macos-aarch64",
        "windows-x86_64",
        "windows-aarch64");

    @Override
    public void apply(Project project) {
        // The source sets we wire into come from the java plugin. Apply it so
        // the one-line `plugins { id("com.boundaryml.baml") }` snippet just works
        // (idempotent if java / java-library / application is already applied).
        project.getPluginManager().apply(JavaPlugin.class);

        BamlExtension extension = project.getExtensions().create(EXTENSION_NAME, BamlExtension.class);
        extension.getSrcDir().convention(project.getLayout().getProjectDirectory());
        extension.getBamlExecutable().convention("baml");
        extension.getOutputType().convention("java");
        extension.getManageDependencies().convention(true);
        // nativePlatforms defaults to empty → auto-detect the build machine.

        ProviderFactory providers = project.getProviders();
        Provider<String> executable = extension.getBamlExecutable();

        TaskProvider<GenerateBamlTask> generateBaml = project.getTasks().register(
            TASK_NAME, GenerateBamlTask.class, task -> {
                task.setGroup("baml");
                task.setDescription(
                    "Generates the typed BAML Java SDK from baml_src/ into "
                        + "build/" + OUTPUT_DIR + ".");
                task.getBamlExecutable().set(executable);
                // Version resolved lazily at input-snapshot time (execution
                // phase), never at configuration. Empty when the CLI is missing.
                task.getBamlVersion().set(
                    providers.provider(() -> resolveVersion(executable.get())));
                task.getSrcDir().set(extension.getSrcDir());
                task.getBamlToml().set(extension.getSrcDir().file("baml.toml"));
                task.getBamlSources().from(extension.getSrcDir().dir("baml_src"));
                task.getOutputDir().set(
                    project.getLayout().getBuildDirectory().dir(OUTPUT_DIR));
            });

        Provider<Directory> outputDir = generateBaml.flatMap(GenerateBamlTask::getOutputDir);

        JavaPluginExtension java = project.getExtensions().getByType(JavaPluginExtension.class);
        SourceSet main = java.getSourceSets().getByName(SourceSet.MAIN_SOURCE_SET_NAME);

        // Generated `.java` (under <outputDir>/baml_sdk/, package baml_sdk) — the
        // source root is the parent of baml_sdk/, i.e. <outputDir>. Register the
        // TASK PROVIDER (not the bare directory) so Gradle infers the
        // generateBaml→compile dependency from the source set itself: `srcDir`
        // resolves a `TaskProvider` to its `@OutputDirectory` and carries that
        // task as the directory's build dependency. This is what makes IntelliJ
        // generate the sources on Gradle *sync* (it reads the source-set model,
        // which now points at a task output), where a plain directory provider
        // would leave the generated root empty until an explicit build. The
        // explicit `compileJava.dependsOn(generateBaml)` below is kept as a
        // belt-and-suspenders (now redundant, harmless).
        main.getJava().srcDir(generateBaml);

        // The bytecode resource (baml_sdk/inlinedbaml.b64) must ride on the
        // runtime classpath at /baml_sdk/inlinedbaml.b64. Scope the include to
        // this `from` spec so the consumer's own resources are untouched.
        project.getTasks().named(
            main.getProcessResourcesTaskName(), ProcessResources.class, task -> {
                task.dependsOn(generateBaml);
                task.from(outputDir, spec -> spec.include("baml_sdk/**/*.b64"));
            });

        project.getTasks().named(main.getCompileJavaTaskName())
            .configure(task -> task.dependsOn(generateBaml));

        // Auto-manage the BAML runtime dependencies. Deferred to afterEvaluate:
        // the injection reads (a) extension values the consumer sets in their
        // `baml { ... }` block and (b) a scan of the consumer's own declared
        // dependencies (to defer to an explicit baml-bridge) — both are only
        // known once the build script has finished evaluating. A configuration-
        // time lazy provider (addAllLater) would instead have to read the
        // dependency set while it is being resolved, which is fragile; the
        // consumer-facing behaviour (dependencies declared, never eagerly
        // resolved) is identical.
        project.afterEvaluate(p -> manageDependencies(p, extension));
    }

    /**
     * Injects the BAML runtime dependencies unless the consumer opted out
     * ({@code manageDependencies = false}) or already declares an explicit
     * {@code com.boundaryml:baml-bridge} dependency (in which case we defer to
     * it and log at info).
     *
     * <p>When the consumer also applies the Kotlin JVM plugin
     * ({@code org.jetbrains.kotlin.jvm}), the Kotlin ergonomics layer
     * {@code com.boundaryml:baml-bridge-kotlin} is injected too (same version,
     * same defer-to-explicit rule); a pure-Java consumer never gets it.
     */
    private static void manageDependencies(Project project, BamlExtension extension) {
        if (!extension.getManageDependencies().getOrElse(true)) {
            project.getLogger().info(
                "baml: manageDependencies = false; not injecting the baml-bridge runtime "
                    + "(you own the com.boundaryml:baml-bridge dependencies).");
            return;
        }
        if (hasExplicitBridgeDependency(project)) {
            project.getLogger().info(
                "baml: found an explicit com.boundaryml:baml-bridge dependency; deferring to it "
                    + "and not injecting the managed runtime.");
            return;
        }

        String version = pluginVersion();
        DependencyHandler dependencies = project.getDependencies();

        // The runtime library on the compile + runtime classpath (the generated
        // SDK compiles against it). Version-locked to the plugin's own version:
        // the plugin and baml-bridge publish from one pipeline at one version.
        dependencies.add(
            JavaPlugin.IMPLEMENTATION_CONFIGURATION_NAME,
            BRIDGE_GROUP + ":" + BRIDGE_ARTIFACT + ":" + version);

        // The per-platform native jar(s), runtime-only (the cdylib is loaded, not
        // compiled against). String notation `group:name:version:classifier`
        // attaches the classifier artifact, matching how a consumer would write
        // runtimeOnly("...:natives-<platform>") by hand.
        for (String platform : resolveNativePlatforms(extension)) {
            dependencies.add(
                JavaPlugin.RUNTIME_ONLY_CONFIGURATION_NAME,
                BRIDGE_GROUP + ":" + BRIDGE_ARTIFACT + ":" + version + ":natives-" + platform);
        }

        // Kotlin consumers additionally get the ergonomics layer
        // (com.boundaryml:baml-bridge-kotlin) at the plugin's own version — same
        // version-lock + defer-to-explicit rules as baml-bridge. Gated on the
        // Kotlin JVM plugin being applied, so a pure-Java consumer never pulls a
        // Kotlin runtime.
        if (project.getPluginManager().hasPlugin(KOTLIN_JVM_PLUGIN_ID)
            && !hasExplicitDependency(project, KOTLIN_ARTIFACT)) {
            dependencies.add(
                JavaPlugin.IMPLEMENTATION_CONFIGURATION_NAME,
                BRIDGE_GROUP + ":" + KOTLIN_ARTIFACT + ":" + version);
        }
    }

    /**
     * Resolve which native-jar platform classifiers to depend on, from the
     * extension's {@code nativePlatforms}:
     *
     * <ul>
     *   <li>empty (the default) → auto-detect the build machine ({@link #detectPlatform()});
     *   <li>a list containing {@code "all"} → the full {@link #ALL_PLATFORMS} set;
     *   <li>any other explicit list → those platforms verbatim (deduped, order preserved).
     * </ul>
     */
    static List<String> resolveNativePlatforms(BamlExtension extension) {
        List<String> configured = extension.getNativePlatforms().getOrElse(List.of());
        if (configured.isEmpty()) {
            return List.of(detectPlatform());
        }
        boolean requestedAll = configured.stream()
            .anyMatch(p -> "all".equalsIgnoreCase(p == null ? "" : p.trim()));
        if (requestedAll) {
            return ALL_PLATFORMS;
        }
        // Explicit list: dedupe (a repeated classifier would add a duplicate
        // artifact) while preserving the consumer's order.
        Set<String> ordered = new LinkedHashSet<>();
        for (String p : configured) {
            if (p != null && !p.trim().isEmpty()) {
                ordered.add(p.trim());
            }
        }
        return new ArrayList<>(ordered);
    }

    /**
     * True if any configuration already declares a {@code com.boundaryml:baml-bridge}
     * dependency (regardless of version/classifier/configuration).
     */
    private static boolean hasExplicitBridgeDependency(Project project) {
        return hasExplicitDependency(project, BRIDGE_ARTIFACT);
    }

    /**
     * True if any configuration already declares a {@code com.boundaryml:<artifact>}
     * dependency (regardless of version/classifier/configuration). Reads only
     * declared dependencies — it never triggers resolution.
     */
    private static boolean hasExplicitDependency(Project project, String artifact) {
        for (Configuration configuration : project.getConfigurations()) {
            for (Dependency dependency : configuration.getDependencies()) {
                if (BRIDGE_GROUP.equals(dependency.getGroup())
                    && artifact.equals(dependency.getName())) {
                    return true;
                }
            }
        }
        return false;
    }

    // ---- plugin-version detection --------------------------------------------

    private static final String VERSION_RESOURCE = "baml-plugin.properties";
    private static final String FALLBACK_VERSION = "0.0.0-dev";

    /**
     * The plugin's own published version — the version at which it injects
     * {@code com.boundaryml:baml-bridge}. Read from a generated classpath
     * resource ({@code baml-plugin.properties}, written by the plugin's own
     * build from {@code -PbamlVersion}); this works under Gradle TestKit, where
     * the plugin is loaded from a local {@code build/} classpath and so has no
     * jar manifest to read {@code Implementation-Version} from. Falls back to
     * the jar manifest, then to {@value #FALLBACK_VERSION}.
     */
    static String pluginVersion() {
        try (InputStream in = BamlPlugin.class.getResourceAsStream(VERSION_RESOURCE)) {
            if (in != null) {
                Properties props = new Properties();
                props.load(in);
                String version = props.getProperty("version");
                if (version != null && !version.isBlank()) {
                    return version.trim();
                }
            }
        } catch (IOException ignored) {
            // Fall through to the manifest / fallback.
        }
        Package pkg = BamlPlugin.class.getPackage();
        String implVersion = pkg == null ? null : pkg.getImplementationVersion();
        if (implVersion != null && !implVersion.isBlank()) {
            return implVersion.trim();
        }
        return FALLBACK_VERSION;
    }

    // ---- platform detection (mirrors baml_bridge NativeLibraryLoader) --------

    /**
     * The {@code <os>-<arch>} native classifier token for the build machine —
     * e.g. {@code linux-x86_64}. Mirrors {@code baml_bridge.internal
     * .NativeLibraryLoader.platformDir()} exactly ({@link #mapOs} + {@link #mapArch})
     * so the jar this depends on is the one the runtime loader will look for.
     */
    static String detectPlatform() {
        return mapOs(System.getProperty("os.name")) + "-" + mapArch(System.getProperty("os.arch"));
    }

    /**
     * Map an {@code os.name} value to {@code linux} / {@code macos} /
     * {@code windows}. Mirrors {@code NativeLibraryLoader.mapOs} (macOS first —
     * "darwin" contains the substring "win").
     */
    static String mapOs(String osName) {
        String n = osName == null ? "" : osName.toLowerCase(Locale.ROOT);
        if (n.contains("mac") || n.contains("darwin") || n.contains("osx")) {
            return "macos";
        }
        if (n.contains("win")) {
            return "windows";
        }
        if (n.contains("linux") || n.contains("nix") || n.contains("nux")) {
            return "linux";
        }
        return n.replaceAll("[^a-z0-9]+", "");
    }

    /**
     * Map an {@code os.arch} value to {@code x86_64} / {@code aarch64}. Mirrors
     * {@code NativeLibraryLoader.mapArch} ({@code amd64}/{@code x64}→{@code x86_64},
     * {@code arm64}/{@code aarch64}→{@code aarch64}).
     */
    static String mapArch(String osArch) {
        String a = osArch == null ? "" : osArch.toLowerCase(Locale.ROOT);
        switch (a) {
            case "amd64":
            case "x86_64":
            case "x64":
                return "x86_64";
            case "aarch64":
            case "arm64":
                return "aarch64";
            default:
                return a.replaceAll("[^a-z0-9]+", "");
        }
    }

    /**
     * Runs {@code <executable> --version} and returns its trimmed stdout, or an
     * empty string if the executable is missing or exits non-zero. Never throws:
     * the empty-string sentinel is what makes {@link GenerateBamlTask} fail at
     * execution (with an install hint) rather than at configuration.
     */
    static String resolveVersion(String executable) {
        try {
            Process process = new ProcessBuilder(executable, "--version")
                .redirectError(ProcessBuilder.Redirect.DISCARD)
                .start();
            byte[] stdout;
            try (var in = process.getInputStream()) {
                stdout = in.readAllBytes();
            }
            if (!process.waitFor(60, TimeUnit.SECONDS)) {
                process.destroyForcibly();
                return "";
            }
            if (process.exitValue() != 0) {
                return "";
            }
            return new String(stdout, StandardCharsets.UTF_8).trim();
        } catch (Exception e) {
            // IOException (command not found) / InterruptedException / etc.
            return "";
        }
    }
}
