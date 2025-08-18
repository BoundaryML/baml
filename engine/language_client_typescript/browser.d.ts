/**
 * @warning This file is intended for browser usage only.
 * For Node.js environments, import Image and Audio directly from '@boundaryml/baml'.
 * Example:
 * ```ts
 * // ✅ Browser usage
 * import { Image, Audio } from '@boundaryml/baml/browser'
 *
 * // ❌ Don't import these from '@boundaryml/baml' in browser environments
 * import { Image, Audio } from '@boundaryml/baml'
 * ```
 */
import { BamlAudio } from "./audio";
import { BamlImage } from "./image";
import { BamlVideo } from "./video";
import { BamlPdf } from "./pdf";
import type { BamlAudio as BamlAudioType } from "./audio";
import type { BamlImage as BamlImageType } from "./image";
import type { BamlPdf as BamlPdfType } from "./pdf";
import type { BamlVideo as BamlVideoType } from "./video";
declare const ImageImpl: typeof BamlImage;
declare const AudioImpl: typeof BamlAudio;
declare const PdfImpl: typeof BamlPdf;
declare const VideoImpl: typeof BamlVideo;
export type Image = BamlImageType;
export type Audio = BamlAudioType;
export type Pdf = BamlPdfType;
export type Video = BamlVideoType;
export { ImageImpl as Image, AudioImpl as Audio, PdfImpl as Pdf, VideoImpl as Video, };
//# sourceMappingURL=browser.d.ts.map