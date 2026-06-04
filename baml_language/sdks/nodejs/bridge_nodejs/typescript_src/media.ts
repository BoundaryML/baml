import { baml_core } from './proto/baml_cffi.js';
import {
    BamlHandle,
    mediaBase64,
    mediaFile,
    mediaFromBase64,
    mediaFromFile,
    mediaFromUrl,
    mediaMimeType,
    mediaUrl,
    mediaValidate,
} from './native.js';

const BamlHandleType = baml_core.cffi.v1.BamlHandleType;

abstract class BamlMedia {
    protected constructor(
        private readonly handle: BamlHandle,
        private readonly expectedHandleType: number,
    ) {}

    url(): string | null {
        return mediaUrl(this.handle, this.expectedHandleType);
    }

    file(): string | null {
        return mediaFile(this.handle, this.expectedHandleType);
    }

    base64(): string {
        return mediaBase64(this.handle, this.expectedHandleType);
    }

    mimeType(): string | null {
        return mediaMimeType(this.handle, this.expectedHandleType);
    }

    _toHandle(): BamlHandle {
        return this.handle.clone();
    }
}

export class BamlImage extends BamlMedia {
    private static readonly handleType = BamlHandleType.ADT_MEDIA_IMAGE;

    private constructor(handle: BamlHandle) {
        super(handle, BamlImage.handleType);
    }

    static fromUrl(url: string, mimeType?: string | null): BamlImage {
        return new BamlImage(mediaFromUrl(BamlImage.handleType, url, mimeType ?? null));
    }

    static fromFile(file: string, mimeType?: string | null): BamlImage {
        return new BamlImage(mediaFromFile(BamlImage.handleType, file, mimeType ?? null));
    }

    static fromBase64(base64: string, mimeType?: string | null): BamlImage {
        return new BamlImage(mediaFromBase64(BamlImage.handleType, base64, mimeType ?? null));
    }

    static _fromHandle(handle: BamlHandle): BamlImage {
        mediaValidate(handle, BamlImage.handleType);
        return new BamlImage(handle);
    }
}

export class BamlAudio extends BamlMedia {
    private static readonly handleType = BamlHandleType.ADT_MEDIA_AUDIO;

    private constructor(handle: BamlHandle) {
        super(handle, BamlAudio.handleType);
    }

    static fromUrl(url: string, mimeType?: string | null): BamlAudio {
        return new BamlAudio(mediaFromUrl(BamlAudio.handleType, url, mimeType ?? null));
    }

    static fromFile(file: string, mimeType?: string | null): BamlAudio {
        return new BamlAudio(mediaFromFile(BamlAudio.handleType, file, mimeType ?? null));
    }

    static fromBase64(base64: string, mimeType?: string | null): BamlAudio {
        return new BamlAudio(mediaFromBase64(BamlAudio.handleType, base64, mimeType ?? null));
    }

    static _fromHandle(handle: BamlHandle): BamlAudio {
        mediaValidate(handle, BamlAudio.handleType);
        return new BamlAudio(handle);
    }
}

export class BamlVideo extends BamlMedia {
    private static readonly handleType = BamlHandleType.ADT_MEDIA_VIDEO;

    private constructor(handle: BamlHandle) {
        super(handle, BamlVideo.handleType);
    }

    static fromUrl(url: string, mimeType?: string | null): BamlVideo {
        return new BamlVideo(mediaFromUrl(BamlVideo.handleType, url, mimeType ?? null));
    }

    static fromFile(file: string, mimeType?: string | null): BamlVideo {
        return new BamlVideo(mediaFromFile(BamlVideo.handleType, file, mimeType ?? null));
    }

    static fromBase64(base64: string, mimeType?: string | null): BamlVideo {
        return new BamlVideo(mediaFromBase64(BamlVideo.handleType, base64, mimeType ?? null));
    }

    static _fromHandle(handle: BamlHandle): BamlVideo {
        mediaValidate(handle, BamlVideo.handleType);
        return new BamlVideo(handle);
    }
}

export class BamlPdf extends BamlMedia {
    private static readonly handleType = BamlHandleType.ADT_MEDIA_PDF;

    private constructor(handle: BamlHandle) {
        super(handle, BamlPdf.handleType);
    }

    static fromUrl(url: string, mimeType?: string | null): BamlPdf {
        return new BamlPdf(mediaFromUrl(BamlPdf.handleType, url, mimeType ?? null));
    }

    static fromFile(file: string, mimeType?: string | null): BamlPdf {
        return new BamlPdf(mediaFromFile(BamlPdf.handleType, file, mimeType ?? null));
    }

    static fromBase64(base64: string, mimeType?: string | null): BamlPdf {
        return new BamlPdf(mediaFromBase64(BamlPdf.handleType, base64, mimeType ?? null));
    }

    static _fromHandle(handle: BamlHandle): BamlPdf {
        mediaValidate(handle, BamlPdf.handleType);
        return new BamlPdf(handle);
    }
}
