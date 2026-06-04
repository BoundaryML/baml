/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
import { BamlHandle } from './native.js';
declare abstract class BamlMedia {
    private readonly handle;
    private readonly expectedHandleType;
    protected constructor(handle: BamlHandle, expectedHandleType: number);
    url(): string | null;
    file(): string | null;
    base64(): string;
    mimeType(): string | null;
    _toHandle(): BamlHandle;
}
export declare class BamlImage extends BamlMedia {
    private static readonly handleType;
    private constructor();
    static fromUrl(url: string, mimeType?: string | null): BamlImage;
    static fromFile(file: string, mimeType?: string | null): BamlImage;
    static fromBase64(base64: string, mimeType?: string | null): BamlImage;
    static _fromHandle(handle: BamlHandle): BamlImage;
}
export declare class BamlAudio extends BamlMedia {
    private static readonly handleType;
    private constructor();
    static fromUrl(url: string, mimeType?: string | null): BamlAudio;
    static fromFile(file: string, mimeType?: string | null): BamlAudio;
    static fromBase64(base64: string, mimeType?: string | null): BamlAudio;
    static _fromHandle(handle: BamlHandle): BamlAudio;
}
export declare class BamlVideo extends BamlMedia {
    private static readonly handleType;
    private constructor();
    static fromUrl(url: string, mimeType?: string | null): BamlVideo;
    static fromFile(file: string, mimeType?: string | null): BamlVideo;
    static fromBase64(base64: string, mimeType?: string | null): BamlVideo;
    static _fromHandle(handle: BamlHandle): BamlVideo;
}
export declare class BamlPdf extends BamlMedia {
    private static readonly handleType;
    private constructor();
    static fromUrl(url: string, mimeType?: string | null): BamlPdf;
    static fromFile(file: string, mimeType?: string | null): BamlPdf;
    static fromBase64(base64: string, mimeType?: string | null): BamlPdf;
    static _fromHandle(handle: BamlHandle): BamlPdf;
}
export {};
//# sourceMappingURL=media.d.ts.map