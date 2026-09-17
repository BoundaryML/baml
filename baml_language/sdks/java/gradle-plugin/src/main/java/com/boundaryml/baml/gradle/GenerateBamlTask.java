package com.boundaryml.baml.gradle;

import java.io.File;
import javax.inject.Inject;
import org.gradle.api.DefaultTask;
import org.gradle.api.GradleException;
import org.gradle.api.file.ConfigurableFileCollection;
import org.gradle.api.file.DirectoryProperty;
import org.gradle.api.file.FileSystemOperations;
import org.gradle.api.file.RegularFileProperty;
import org.gradle.api.provider.Property;
import org.gradle.api.tasks.CacheableTask;
import org.gradle.api.tasks.Input;
import org.gradle.api.tasks.InputFile;
import org.gradle.api.tasks.InputFiles;
import org.gradle.api.tasks.Internal;
import org.gradle.api.tasks.OutputDirectory;
import org.gradle.api.tasks.PathSensitive;
import org.gradle.api.tasks.PathSensitivity;
import org.gradle.api.tasks.TaskAction;
import org.gradle.process.ExecOperations;

/**
 * Runs {@code baml bridge generate} to produce the typed BAML Java SDK.
 *
 * <p>Cacheable and incremental: the declared inputs are {@code baml.toml}, every
 * file under {@code baml_src/}, and the resolved CLI version string; the output
 * is {@code build/generated/sources/baml/java/main}. Gradle skips the task as
 * {@code UP-TO-DATE} when none of those changed.
 *
 * <p>The output directory is cleaned before each generation (BAML's own
 * {@code bridge generate} does not remove stale files — a renamed/deleted BAML class
 * would otherwise leave a stale {@code .java} that still compiles).
 */
@CacheableTask
public abstract class GenerateBamlTask extends DefaultTask {

    /** The {@code baml} executable (bare name resolved on PATH, or abs path). */
    @Internal
    public abstract Property<String> getBamlExecutable();

    /** The project directory passed to the CLI as {@code --project}. */
    @Internal
    public abstract DirectoryProperty getSrcDir();

    /** {@code baml.toml} — a declared input so config changes retrigger generation. */
    @InputFile
    @PathSensitive(PathSensitivity.RELATIVE)
    public abstract RegularFileProperty getBamlToml();

    /** Every file under {@code baml_src/} — the BAML sources. */
    @InputFiles
    @PathSensitive(PathSensitivity.RELATIVE)
    public abstract ConfigurableFileCollection getBamlSources();

    /**
     * The resolved CLI version (from {@code baml --version}). An {@code @Input}
     * so a toolchain bump retriggers generation. Wired by {@link BamlPlugin} to
     * a provider that returns an empty string when the executable is missing or
     * fails to run — in which case {@link #generate()} fails with an install
     * hint (never at configuration time).
     */
    @Input
    public abstract Property<String> getBamlVersion();

    /** {@code build/generated/sources/baml/java/main} — the generated tree root. */
    @OutputDirectory
    public abstract DirectoryProperty getOutputDir();

    @Inject
    public abstract ExecOperations getExecOperations();

    @Inject
    public abstract FileSystemOperations getFileSystemOperations();

    @TaskAction
    public void generate() {
        String executable = getBamlExecutable().get();

        // Empty version = the CLI could not be run at input-snapshot time. Fail
        // the TASK (execution phase) with a clear install hint, not the config.
        String version = getBamlVersion().getOrElse("");
        if (version.isBlank()) {
            throw new GradleException(missingExecutableMessage(executable));
        }

        // A bare executable name is resolved against PATH ourselves (Gradle's
        // exec does not reliably search PATH for the child process), so the
        // default `bamlExecutable = "baml"` works. Path-qualified names are
        // handed to Gradle verbatim (it resolves them against the project dir).
        String resolvedExecutable = executable;
        if (!isPathQualified(executable)) {
            File onPath = findOnPath(executable);
            if (onPath == null) {
                throw new GradleException(missingExecutableMessage(executable));
            }
            resolvedExecutable = onPath.getAbsolutePath();
        }
        final String executablePath = resolvedExecutable;

        File outputRoot = getOutputDir().get().getAsFile();

        // Clean the output dir before generation (stale-file hazard: generate
        // does not remove files for renamed/deleted BAML classes).
        getFileSystemOperations().delete(spec -> spec.delete(outputRoot));

        // Generated files declare `package baml_sdk.*`, so the emitter's output
        // root must be a `baml_sdk/` directory whose parent is the source root.
        // With `-o`, the CLI writes verbatim (no auto-appended `baml_sdk`), so
        // point it at <outputRoot>/baml_sdk and register <outputRoot> as the
        // source root (done by BamlPlugin).
        File sdkDir = new File(outputRoot, "baml_sdk");
        //noinspection ResultOfMethodCallIgnored
        sdkDir.mkdirs();

        File project = getSrcDir().get().getAsFile();

        getExecOperations().exec(spec -> {
            spec.setExecutable(executablePath);
            spec.args(
                "bridge", "generate",
                "--project", project.getAbsolutePath(),
                "-o", sdkDir.getAbsolutePath());
        });
    }

    private static boolean isPathQualified(String executable) {
        return new File(executable).isAbsolute() || executable.contains(File.separator);
    }

    /** Locates a bare executable name on {@code PATH}, or {@code null} if absent. */
    private static File findOnPath(String executable) {
        String path = System.getenv("PATH");
        if (path == null) {
            return null;
        }
        for (String dir : path.split(File.pathSeparator)) {
            if (dir.isEmpty()) {
                continue;
            }
            File candidate = new File(dir, executable);
            if (candidate.isFile() && candidate.canExecute()) {
                return candidate;
            }
        }
        return null;
    }

    private static String missingExecutableMessage(String executable) {
        return "BAML executable '" + executable + "' was not found (or failed to run).\n"
            + "\n"
            + "Install the BAML CLI, e.g.:\n"
            + "    curl -fsSL https://pkg.boundaryml.com/install.sh | sh\n"
            + "\n"
            + "(see scripts/install.sh), then re-run the build. Alternatively set the\n"
            + "executable explicitly in your build script:\n"
            + "\n"
            + "    baml {\n"
            + "        bamlExecutable.set(\"/absolute/path/to/baml\")\n"
            + "    }\n"
            + "\n"
            + "or add the `baml` binary to your PATH.";
    }
}
