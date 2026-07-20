package com.boundaryml.baml.gradle;

import java.nio.charset.StandardCharsets;
import java.util.concurrent.TimeUnit;
import org.gradle.api.Plugin;
import org.gradle.api.Project;
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
 *       {@code compileJava} / {@code processResources} depend on it.
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
