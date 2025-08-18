/**
 * Browser-compatible implementation of BamlVideo
 */
export declare class BamlVideo {
    private readonly type;
    private readonly content;
    private readonly mediaType?;
    private constructor();
    /**
     * Create a BamlVideo from a URL
     */
    static fromUrl(url: string, mediaType?: string): BamlVideo;
    /**
     * Create a BamlVideo from base64 encoded data
     */
    static fromBase64(mediaType: string, base64: string): BamlVideo;
    /**
     * Create a BamlVideo from a File object
     */
    static fromFile(file: File): Promise<BamlVideo>;
    /**
     * Create a BamlVideo from a Blob object
     */
    static fromBlob(blob: Blob, mediaType?: string): Promise<BamlVideo>;
    /**
     * Create a BamlVideo by fetching from a URL
     */
    static fromUrlAsync(url: string): Promise<BamlVideo>;
    /**
     * Check if the video is stored as a URL
     */
    isUrl(): boolean;
    /**
     * Get the URL of the video if it's stored as a URL
     * @throws Error if the video is not stored as a URL
     */
    asUrl(): string;
    /**
     * Get the base64 data and media type if the video is stored as base64
     * @returns [base64Data, mediaType]
     * @throws Error if the video is not stored as base64
     */
    asBase64(): [string, string];
    /**
     * Convert the video to a JSON representation
     */
    toJSON(): {
        url: string;
    } | {
        base64: string;
        media_type: string;
    };
}
//# sourceMappingURL=video.d.ts.map