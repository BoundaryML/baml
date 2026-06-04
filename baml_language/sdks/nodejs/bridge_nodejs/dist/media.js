/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
import { baml_core } from './proto/baml_cffi.js';
import { mediaBase64, mediaFile, mediaFromBase64, mediaFromFile, mediaFromUrl, mediaMimeType, mediaUrl, mediaValidate, } from './native.js';
const BamlHandleType = baml_core.cffi.v1.BamlHandleType;
class BamlMedia {
    handle;
    expectedHandleType;
    constructor(handle, expectedHandleType) {
        this.handle = handle;
        this.expectedHandleType = expectedHandleType;
    }
    url() {
        return mediaUrl(this.handle, this.expectedHandleType);
    }
    file() {
        return mediaFile(this.handle, this.expectedHandleType);
    }
    base64() {
        return mediaBase64(this.handle, this.expectedHandleType);
    }
    mimeType() {
        return mediaMimeType(this.handle, this.expectedHandleType);
    }
    _toHandle() {
        return this.handle.clone();
    }
}
export class BamlImage extends BamlMedia {
    static handleType = BamlHandleType.ADT_MEDIA_IMAGE;
    constructor(handle) {
        super(handle, BamlImage.handleType);
    }
    static fromUrl(url, mimeType) {
        return new BamlImage(mediaFromUrl(BamlImage.handleType, url, mimeType ?? null));
    }
    static fromFile(file, mimeType) {
        return new BamlImage(mediaFromFile(BamlImage.handleType, file, mimeType ?? null));
    }
    static fromBase64(base64, mimeType) {
        return new BamlImage(mediaFromBase64(BamlImage.handleType, base64, mimeType ?? null));
    }
    static _fromHandle(handle) {
        mediaValidate(handle, BamlImage.handleType);
        return new BamlImage(handle);
    }
}
export class BamlAudio extends BamlMedia {
    static handleType = BamlHandleType.ADT_MEDIA_AUDIO;
    constructor(handle) {
        super(handle, BamlAudio.handleType);
    }
    static fromUrl(url, mimeType) {
        return new BamlAudio(mediaFromUrl(BamlAudio.handleType, url, mimeType ?? null));
    }
    static fromFile(file, mimeType) {
        return new BamlAudio(mediaFromFile(BamlAudio.handleType, file, mimeType ?? null));
    }
    static fromBase64(base64, mimeType) {
        return new BamlAudio(mediaFromBase64(BamlAudio.handleType, base64, mimeType ?? null));
    }
    static _fromHandle(handle) {
        mediaValidate(handle, BamlAudio.handleType);
        return new BamlAudio(handle);
    }
}
export class BamlVideo extends BamlMedia {
    static handleType = BamlHandleType.ADT_MEDIA_VIDEO;
    constructor(handle) {
        super(handle, BamlVideo.handleType);
    }
    static fromUrl(url, mimeType) {
        return new BamlVideo(mediaFromUrl(BamlVideo.handleType, url, mimeType ?? null));
    }
    static fromFile(file, mimeType) {
        return new BamlVideo(mediaFromFile(BamlVideo.handleType, file, mimeType ?? null));
    }
    static fromBase64(base64, mimeType) {
        return new BamlVideo(mediaFromBase64(BamlVideo.handleType, base64, mimeType ?? null));
    }
    static _fromHandle(handle) {
        mediaValidate(handle, BamlVideo.handleType);
        return new BamlVideo(handle);
    }
}
export class BamlPdf extends BamlMedia {
    static handleType = BamlHandleType.ADT_MEDIA_PDF;
    constructor(handle) {
        super(handle, BamlPdf.handleType);
    }
    static fromUrl(url, mimeType) {
        return new BamlPdf(mediaFromUrl(BamlPdf.handleType, url, mimeType ?? null));
    }
    static fromFile(file, mimeType) {
        return new BamlPdf(mediaFromFile(BamlPdf.handleType, file, mimeType ?? null));
    }
    static fromBase64(base64, mimeType) {
        return new BamlPdf(mediaFromBase64(BamlPdf.handleType, base64, mimeType ?? null));
    }
    static _fromHandle(handle) {
        mediaValidate(handle, BamlPdf.handleType);
        return new BamlPdf(handle);
    }
}
//# sourceMappingURL=media.js.map