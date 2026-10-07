// `fromFile` on the media classes.
//
// The media classes are native (`native.d.ts`), and every other constructor
// builds its value without the engine. `fromFile` reads a file, which is what
// BAML's `from_file` does: calling that function keeps one way of reading and
// one error for a file that cannot be read, here and in BAML code.

import { BamlAudio, BamlImage, BamlPdf, BamlVideo } from './native.js';
import { invokeTarget } from './proto.js';

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

function fromFile<Media>(classFqn: string): (file: string, mimeType?: string | null) => Media {
    return (file, mimeType) =>
        invokeTarget(`${classFqn}.from_file`, { file, mime_type: mimeType ?? null }, undefined, false) as Media;
}

BamlImage.fromFile = fromFile<BamlImage>('baml.media.Image');
BamlAudio.fromFile = fromFile<BamlAudio>('baml.media.Audio');
BamlVideo.fromFile = fromFile<BamlVideo>('baml.media.Video');
BamlPdf.fromFile = fromFile<BamlPdf>('baml.media.Pdf');
