/**
 * Browser-compatible implementation of BamlPdf
 */
export declare class BamlPdf {
    private readonly type;
    private readonly content;
    private readonly mediaType?;
    private constructor();
    /**
     * Create a BamlPdf from a URL
     */
    static fromUrl(url: string, mediaType?: string): BamlPdf;
    /**
     * Create a BamlPdf from base64 encoded data
     */
    static fromBase64(mediaType: string, base64: string): BamlPdf;
    /**
     * Create a BamlPdf from a File object
     */
    static fromFile(file: File): Promise<BamlPdf>;
    /**
     * Create a BamlPdf from a Blob object
     */
    static fromBlob(blob: Blob, mediaType?: string): Promise<BamlPdf>;
    /**
     * Create a BamlPdf by fetching from a URL
     */
    static fromUrlAsync(url: string): Promise<BamlPdf>;
    /**
     * Check if the pdf is stored as a URL
     */
    isUrl(): boolean;
    /**
     * Get the URL of the pdf if it's stored as a URL
     * @throws Error if the pdf is not stored as a URL
     */
    asUrl(): string;
    /**
     * Get the base64 data and media type if the pdf is stored as base64
     * @returns [base64Data, mediaType]
     * @throws Error if the pdf is not stored as base64
     */
    asBase64(): [string, string];
    /**
     * Convert the pdf to a JSON representation
     */
    toJSON(): {
        url: string;
    } | {
        base64: string;
        media_type: string;
    };
}
//# sourceMappingURL=pdf.d.ts.map