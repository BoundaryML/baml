package com.boundaryml.baml.gradle;

import org.gradle.api.file.DirectoryProperty;
import org.gradle.api.provider.ListProperty;
import org.gradle.api.provider.Property;

/**
 * Configuration for the BAML Gradle plugin, exposed as the {@code baml { ... }}
 * block:
 *
 * <pre>
 * baml {
 *     srcDir.set(layout.projectDirectory)      // where baml.toml + baml_src/ live
 *     bamlExecutable.set("baml")               // the CLI to run (PATH or abs path)
 *     outputType.set("java")                   // informational only
 *     nativePlatforms.set(listOf("all"))       // which native jars to depend on
 *     manageDependencies.set(true)             // auto-add the baml-bridge runtime
 * }
 * </pre>
 *
 * <p>Every option has a convention (default) applied by {@link BamlPlugin}, so
 * the block is optional for the common case — the whole point of the plugin is
 * that {@code plugins { id("com.boundaryml.baml") version "X" }} is the entire
 * setup (it injects the matching {@code com.boundaryml:baml-bridge} runtime and
 * the native jar for the build machine automatically).
 */
public abstract class BamlExtension {

    /**
     * Project directory containing {@code baml.toml} and {@code baml_src/}.
     * Passed to the CLI as {@code --from}. Default: the project directory.
     */
    public abstract DirectoryProperty getSrcDir();

    /**
     * The {@code baml} executable to invoke. May be a bare name resolved on the
     * {@code PATH} (the default, {@code "baml"}) or an absolute path to a
     * specific binary. The CLI owns toolchain/version resolution.
     */
    public abstract Property<String> getBamlExecutable();

    /**
     * Informational only: the generator output type. The real generator
     * configuration (output type, naming convention, output dir) lives in
     * {@code baml.toml} under {@code [generator.<name>]}. Default: {@code "java"}.
     */
    public abstract Property<String> getOutputType();

    /**
     * Which per-platform native jars to depend on (the classifier tokens
     * {@code linux-x86_64}, {@code linux-aarch64}, {@code macos-x86_64},
     * {@code macos-aarch64}, {@code windows-x86_64}, {@code windows-aarch64}).
     *
     * <p><b>Default (empty):</b> the plugin auto-detects the build machine's
     * platform from {@code os.name}/{@code os.arch} and depends only on that one
     * native jar — the zero-config path.
     *
     * <p><b>Explicit list:</b> setting a non-empty list <em>replaces</em>
     * detection with exactly the platforms you name — e.g. to build on Linux but
     * run on macOS, or to ship a runnable artifact for several targets.
     *
     * <p><b>The special value {@code "all"}:</b> a list containing {@code "all"}
     * expands to every platform in the known set. This is always safe:
     * {@code baml_bridge}'s {@code NativeLibraryLoader} selects the jar to load
     * by the JVM's own {@code os.name}/{@code os.arch} at runtime (first-hit-wins
     * on the {@code /native/<os>-<arch>/} classpath resource), so the extra
     * native jars for other platforms are inert — they simply sit unused on the
     * runtime classpath. The trade-off is download size (one engine cdylib per
     * platform), not correctness.
     *
     * <p>Note: the experimental musl classifiers ({@code linux-*-musl}) are never
     * auto-detected and are not part of {@code "all"}; a musl (Alpine) consumer
     * must request that classifier explicitly.
     */
    public abstract ListProperty<String> getNativePlatforms();

    /**
     * Whether the plugin auto-manages the BAML runtime dependencies. Default:
     * {@code true}.
     *
     * <p>When {@code true}, the plugin injects
     * {@code implementation("com.boundaryml:baml-bridge:<pluginVersion>")} plus a
     * {@code runtimeOnly} native jar per resolved platform (see
     * {@link #getNativePlatforms()}) — unless the project already declares an
     * explicit {@code com.boundaryml:baml-bridge} dependency, in which case the
     * plugin defers to it and injects nothing.
     *
     * <p>Set to {@code false} to opt out entirely — the escape hatch for exotic
     * setups (a custom runtime coordinate, a vendored jar, a platform the
     * standard classifiers don't cover). You then own the {@code baml-bridge}
     * (and native jar) dependencies yourself.
     */
    public abstract Property<Boolean> getManageDependencies();
}
