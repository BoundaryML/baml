/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
declare module './native.js' {
    namespace BamlImage {
        /**
         * Read the file now: the value holds its content and its base name,
         * never its path. Throws the BAML error `baml.errors.Io` when the
         * file cannot be read.
         */
        function fromFile(file: string, mimeType?: string | null): BamlImage;
    }
    namespace BamlAudio {
        /**
         * Read the file now: the value holds its content and its base name,
         * never its path. Throws the BAML error `baml.errors.Io` when the
         * file cannot be read.
         */
        function fromFile(file: string, mimeType?: string | null): BamlAudio;
    }
    namespace BamlVideo {
        /**
         * Read the file now: the value holds its content and its base name,
         * never its path. Throws the BAML error `baml.errors.Io` when the
         * file cannot be read.
         */
        function fromFile(file: string, mimeType?: string | null): BamlVideo;
    }
    namespace BamlPdf {
        /**
         * Read the file now: the value holds its content and its base name,
         * never its path. Throws the BAML error `baml.errors.Io` when the
         * file cannot be read.
         */
        function fromFile(file: string, mimeType?: string | null): BamlPdf;
    }
}
export {};
//# sourceMappingURL=media.d.ts.map