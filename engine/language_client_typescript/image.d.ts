/**
 * Browser-compatible implementation of BamlImage
 */
export declare class BamlImage {
    private readonly type;
    private readonly content;
    private readonly mediaType?;
    private constructor();
    /**
     * Create a BamlImage from a URL
     */
    static fromUrl(url: string, mediaType?: string): BamlImage;
    /**
     * Create a BamlImage from base64 encoded data
     */
    static fromBase64(mediaType: string, base64: string): BamlImage;
    /**
     * Create a BamlImage from a File object
     */
    static fromFile(file: File): Promise<BamlImage>;
    /**
     * Create a BamlImage from a Blob object
     */
    static fromBlob(blob: Blob, mediaType?: string): Promise<BamlImage>;
    /**
     * Create a BamlImage by fetching from a URL
     */
    static fromUrlAsync(url: string): Promise<BamlImage>;
    /**
     * Check if the image is stored as a URL
     */
    isUrl(): boolean;
    /**
     * Get the URL of the image if it's stored as a URL
     * @throws Error if the image is not stored as a URL
     */
    asUrl(): string;
    /**
     * Get the base64 data and media type if the image is stored as base64
     * @returns [base64Data, mediaType]
     * @throws Error if the image is not stored as base64
     */
    asBase64(): [string, string];
    /**
     * Convert the image to a JSON representation
     */
    toJSON(): {
        url: string;
    } | {
        base64: string;
        media_type: string;
    };
}
//# sourceMappingURL=image.d.ts.map