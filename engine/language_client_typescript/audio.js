"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.BamlAudio = void 0;
/**
 * Browser-compatible implementation of BamlAudio
 */
class BamlAudio {
    type;
    content;
    mediaType;
    constructor(type, content, mediaType) {
        this.type = type;
        this.content = content;
        this.mediaType = mediaType;
    }
    /**
     * Create a BamlAudio from a URL
     */
    static fromUrl(url, mediaType) {
        return new BamlAudio('url', url, mediaType);
    }
    /**
     * Create a BamlAudio from base64 encoded data
     */
    static fromBase64(mediaType, base64) {
        return new BamlAudio('base64', base64, mediaType);
    }
    /**
     * Create a BamlAudio from a File object
     */
    static async fromFile(file) {
        return BamlAudio.fromBlob(file, file.type);
    }
    /**
     * Create a BamlAudio from a Blob object
     */
    static async fromBlob(blob, mediaType) {
        const base64 = await new Promise((resolve, reject) => {
            const reader = new FileReader();
            reader.onload = () => resolve(reader.result);
            reader.onerror = reject;
            reader.readAsDataURL(blob);
        });
        // Remove the data URL prefix to get just the base64 string
        const base64Data = base64.replace(/^data:.*?;base64,/, '');
        return BamlAudio.fromBase64(mediaType || blob.type, base64Data);
    }
    /**
     * Create a BamlAudio by fetching from a URL
     */
    static async fromUrlAsync(url) {
        const response = await fetch(url);
        const blob = await response.blob();
        return BamlAudio.fromBlob(blob);
    }
    /**
     * Check if the audio is stored as a URL
     */
    isUrl() {
        return this.type === 'url';
    }
    /**
     * Get the URL of the audio if it's stored as a URL
     * @throws Error if the audio is not stored as a URL
     */
    asUrl() {
        if (!this.isUrl()) {
            throw new Error('Audio is not a URL');
        }
        return this.content;
    }
    /**
     * Get the base64 data and media type if the audio is stored as base64
     * @returns [base64Data, mediaType]
     * @throws Error if the audio is not stored as base64
     */
    asBase64() {
        if (this.type !== 'base64') {
            throw new Error('Audio is not base64');
        }
        return [this.content, this.mediaType || ''];
    }
    /**
     * Convert the audio to a JSON representation
     */
    toJSON() {
        if (this.type === 'url') {
            return { url: this.content };
        }
        return {
            base64: this.content,
            media_type: this.mediaType || '',
        };
    }
}
exports.BamlAudio = BamlAudio;
