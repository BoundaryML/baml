package baml_bridge.internal;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.Locale;

/**
 * Self-contained loader for the {@code bridge_java} native cdylib. Resolves and
 * {@link System#load(String)}s the library exactly once, following a
 * first-hit-wins ladder:
 *
 * <ol>
 *   <li>the system property named by {@code propertyName} (dev override), then</li>
 *   <li>the environment variable named by {@code envVarName} (dev/test override), then</li>
 *   <li>the bundled classpath resource {@code /native/{os}-{arch}/{libname}} — the
 *       per-platform native jar convention — extracted to a temp file and loaded.</li>
 * </ol>
 *
 * <p>Kept in {@code baml_bridge.internal} — deliberately free of any native
 * static initializer — so the platform-string mapping ({@link #mapOs},
 * {@link #mapArch}, {@link #nativeResourcePath}) and the extraction helper
 * ({@link #extractResource}) can be unit-tested without triggering a real
 * {@code System.load}.
 */
public final class NativeLibraryLoader {
    /** Base name passed to {@link System#mapLibraryName} (→ libbridge_java.so / .dylib / bridge_java.dll). */
    public static final String LIB_BASE_NAME = "bridge_java";

    private NativeLibraryLoader() {}

    /**
     * Resolve and {@link System#load} the native library, walking the three-step
     * ladder. Throws {@link IllegalStateException} listing every attempted source
     * if none resolves.
     *
     * @param propertyName system property consulted first (dev override)
     * @param envVarName   environment variable consulted second (dev/test override)
     */
    public static void load(String propertyName, String envVarName) {
        // (1) dev override: system property.
        String prop = System.getProperty(propertyName);
        if (prop != null && !prop.isEmpty()) {
            System.load(prop);
            return;
        }
        // (2) dev/test override: environment variable.
        String env = System.getenv(envVarName);
        if (env != null && !env.isEmpty()) {
            System.load(env);
            return;
        }
        // (3) bundled per-platform classpath resource.
        String resourcePath = nativeResourcePath();
        File extracted;
        try {
            extracted = extractResource(resourcePath, libFileName());
        } catch (IOException e) {
            throw new IllegalStateException(
                    "failed to extract the bundled bridge_java native library from classpath resource "
                            + resourcePath,
                    e);
        }
        if (extracted != null) {
            System.load(extracted.getAbsolutePath());
            return;
        }

        throw new IllegalStateException(
                "could not locate the bridge_java native library. Tried, in order:\n"
                        + "  (1) system property '" + propertyName + "' (unset or empty)\n"
                        + "  (2) environment variable '" + envVarName + "' (unset or empty)\n"
                        + "  (3) classpath resource '" + resourcePath + "' (not found)\n"
                        + "Fixes: for a dev build pass -D" + propertyName + "=<path to " + libFileName()
                        + "> (e.g. target/release/" + libFileName() + "), or add the matching per-platform "
                        + "native jar com.boundaryml.baml:baml-bridge:<version>:natives-" + platformDir()
                        + " to the classpath.");
    }

    /**
     * Extract the classpath resource at {@code resourcePath} into a fresh
     * {@code baml-bridge-native} temp directory as {@code targetFileName}, marking
     * both the file and its directory {@code deleteOnExit}. Returns {@code null}
     * (does not throw) when the resource is absent from the classpath.
     */
    public static File extractResource(String resourcePath, String targetFileName) throws IOException {
        try (InputStream in = NativeLibraryLoader.class.getResourceAsStream(resourcePath)) {
            if (in == null) {
                return null;
            }
            Path dir = Files.createTempDirectory("baml-bridge-native");
            dir.toFile().deleteOnExit();
            Path out = dir.resolve(targetFileName);
            Files.copy(in, out, StandardCopyOption.REPLACE_EXISTING);
            File file = out.toFile();
            file.deleteOnExit();
            return file;
        }
    }

    /** The absolute classpath resource path for this host: {@code /native/{os}-{arch}/{libname}}. */
    public static String nativeResourcePath() {
        return "/native/" + platformDir() + "/" + libFileName();
    }

    /** {@code {os}-{arch}} directory token for this host (e.g. {@code linux-x86_64}). */
    public static String platformDir() {
        return mapOs(System.getProperty("os.name")) + "-" + mapArch(System.getProperty("os.arch"));
    }

    /** Host library file name, e.g. {@code libbridge_java.so} / {@code libbridge_java.dylib} / {@code bridge_java.dll}. */
    public static String libFileName() {
        return System.mapLibraryName(LIB_BASE_NAME);
    }

    /**
     * Map a {@code os.name} value to the canonical token {@code linux} /
     * {@code macos} / {@code windows}. Unknown values are sanitized (lowercased,
     * non-alphanumerics stripped) so the resulting resource path is still
     * well-formed and shows up usefully in the not-found error.
     */
    public static String mapOs(String osName) {
        String n = osName == null ? "" : osName.toLowerCase(Locale.ROOT);
        // macOS first: "darwin" contains the substring "win", so it must be
        // matched before the Windows check.
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
     * Map a {@code os.arch} value to the canonical token {@code x86_64} /
     * {@code aarch64} ({@code amd64}/{@code x64}→{@code x86_64},
     * {@code arm64}→{@code aarch64}). Unknown values are sanitized.
     */
    public static String mapArch(String osArch) {
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
}
