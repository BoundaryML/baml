/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
// `fromFile` on the media classes.
//
// The media classes are native (`native.d.ts`), and every other constructor
// builds its value without the engine. `fromFile` reads a file, which is what
// BAML's `from_file` does: calling that function keeps one way of reading and
// one error for a file that cannot be read, here and in BAML code.
import { BamlAudio, BamlImage, BamlPdf, BamlVideo } from './native.js';
import { invokeTarget } from './proto.js';
function fromFile(classFqn) {
    return (file, mimeType) => invokeTarget(`${classFqn}.from_file`, { file, mime_type: mimeType ?? null }, undefined, false);
}
BamlImage.fromFile = fromFile('baml.media.Image');
BamlAudio.fromFile = fromFile('baml.media.Audio');
BamlVideo.fromFile = fromFile('baml.media.Video');
BamlPdf.fromFile = fromFile('baml.media.Pdf');
//# sourceMappingURL=media.js.map