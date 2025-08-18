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

// Import actual implementations
import { BamlAudio } from "./audio";
import { BamlImage } from "./image";
import { BamlVideo } from "./video";
import { BamlPdf } from "./pdf";

// Re-export the original types
import type { BamlAudio as BamlAudioType } from "./audio";
import type { BamlImage as BamlImageType } from "./image";
import type { BamlPdf as BamlPdfType } from "./pdf";
import type { BamlVideo as BamlVideoType } from "./video";

// Detect if we're in server-side rendering environment
const isSSR = typeof window === "undefined";

// Create a proxy handler that logs warnings in SSR environment
function createSSRProxyHandler<T extends object>(
  name: string
): ProxyHandler<T> {
  return {
    get: (target, prop) => {
      if (isSSR) {
        console.warn(
          `Using ${name} from '@boundaryml/baml/browser' in a server-side environment. This will not function properly in SSR.`
        );
      }
      return (target as Record<string | symbol, unknown>)[prop];
    },
  };
}

// Create proxied versions that will work in both environments but warn in SSR
const ImageImpl = new Proxy(
  BamlImage,
  createSSRProxyHandler<typeof BamlImage>("Image")
);
const AudioImpl = new Proxy(
  BamlAudio,
  createSSRProxyHandler<typeof BamlAudio>("Audio")
);
const PdfImpl = new Proxy(
  BamlPdf,
  createSSRProxyHandler<typeof BamlPdf>("Pdf")
);
const VideoImpl = new Proxy(
  BamlVideo,
  createSSRProxyHandler<typeof BamlVideo>("Video")
);

// Now export everything properly
// First, define the type alias
export type Image = BamlImageType;
export type Audio = BamlAudioType;
export type Pdf = BamlPdfType;
export type Video = BamlVideoType;

// Then export the implementations
export {
  ImageImpl as Image,
  AudioImpl as Audio,
  PdfImpl as Pdf,
  VideoImpl as Video,
};
