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
import { BamlAudio } from './audio';
import { BamlImage } from './image';
import type { BamlAudio as BamlAudioType } from './audio';
import type { BamlImage as BamlImageType } from './image';
declare const ImageImpl: typeof BamlImage;
declare const AudioImpl: typeof BamlAudio;
export type Image = BamlImageType;
export type Audio = BamlAudioType;
export { ImageImpl as Image, AudioImpl as Audio };
//# sourceMappingURL=browser.d.ts.map