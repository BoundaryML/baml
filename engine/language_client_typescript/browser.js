"use strict";
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
Object.defineProperty(exports, "__esModule", { value: true });
exports.Audio = exports.Image = void 0;
const audio_1 = require("./audio");
// Import actual implementations
const image_1 = require("./image");
// Detect if we're in server-side rendering environment
const isSSR = typeof window === 'undefined';
// Create a proxy handler that logs warnings in SSR environment
function createSSRProxyHandler(name) {
    return {
        get: (target, prop) => {
            if (isSSR) {
                console.warn(`Using ${name} from '@boundaryml/baml/browser' in a server-side environment. This will not function properly in SSR.`);
            }
            return target[prop];
        },
    };
}
// Create proxied versions that will work in both environments but warn in SSR
const ImageImpl = new Proxy(image_1.BamlImage, createSSRProxyHandler('Image'));
exports.Image = ImageImpl;
const AudioImpl = new Proxy(audio_1.BamlAudio, createSSRProxyHandler('Audio'));
exports.Audio = AudioImpl;
