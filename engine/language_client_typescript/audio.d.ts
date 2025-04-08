/**
 * Browser-compatible implementation of BamlAudio
 */
export declare class BamlAudio {
    private readonly type;
    private readonly content;
    private readonly mediaType?;
    private constructor();
    /**
     * Create a BamlAudio from a URL
     */
    static fromUrl(url: string, mediaType?: string): BamlAudio;
    /**
     * Create a BamlAudio from base64 encoded data
     */
    static fromBase64(mediaType: string, base64: string): BamlAudio;
    /**
     * Create a BamlAudio from a File object
     */
    static fromFile(file: File): Promise<BamlAudio>;
    /**
     * Create a BamlAudio from a Blob object
     */
    static fromBlob(blob: Blob, mediaType?: string): Promise<BamlAudio>;
    /**
     * Create a BamlAudio by fetching from a URL
     */
    static fromUrlAsync(url: string): Promise<BamlAudio>;
    /**
     * Check if the audio is stored as a URL
     */
    isUrl(): boolean;
    /**
     * Get the URL of the audio if it's stored as a URL
     * @throws Error if the audio is not stored as a URL
     */
    asUrl(): string;
    /**
     * Get the base64 data and media type if the audio is stored as base64
     * @returns [base64Data, mediaType]
     * @throws Error if the audio is not stored as base64
     */
    asBase64(): [string, string];
    /**
     * Convert the audio to a JSON representation
     */
    toJSON(): {
        url: string;
    } | {
        base64: string;
        media_type: string;
    };
}
//# sourceMappingURL=audio.d.ts.map