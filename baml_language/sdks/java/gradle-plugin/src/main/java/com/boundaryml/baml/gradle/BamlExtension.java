package com.boundaryml.baml.gradle;

import org.gradle.api.file.DirectoryProperty;
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
 * }
 * </pre>
 *
 * <p>Every option has a convention (default) applied by {@link BamlPlugin}, so
 * the block is optional for the common case.
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
}
