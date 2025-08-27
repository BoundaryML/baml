/**
 * Browser-compatible implementation of BamlPdf
 */
export class BamlPdf {
  private constructor(
    private readonly type: "url" | "base64",
    private readonly content: string,
    private readonly mediaType?: string
  ) {}

  /**
   * Create a BamlPdf from a URL
   */
  static fromUrl(url: string, mediaType?: string): BamlPdf {
    return new BamlPdf("url", url, mediaType);
  }

  /**
   * Create a BamlPdf from base64 encoded data
   */
  static fromBase64(mediaType: string, base64: string): BamlPdf {
    return new BamlPdf("base64", base64, mediaType);
  }

  /**
   * Create a BamlPdf from a File object
   */
  static async fromFile(file: File): Promise<BamlPdf> {
    return BamlPdf.fromBlob(file, file.type);
  }

  /**
   * Create a BamlPdf from a Blob object
   */
  static async fromBlob(blob: Blob, mediaType?: string): Promise<BamlPdf> {
    const base64 = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result as string);
      reader.onerror = reject;
      reader.readAsDataURL(blob);
    });
    // Remove the data URL prefix to get just the base64 string
    const base64Data = base64.replace(/^data:.*?;base64,/, "");
    return BamlPdf.fromBase64(mediaType || blob.type, base64Data);
  }

  /**
   * Create a BamlPdf by fetching from a URL
   */
  static async fromUrlAsync(url: string): Promise<BamlPdf> {
    const response = await fetch(url);
    const blob = await response.blob();
    return BamlPdf.fromBlob(blob);
  }

  /**
   * Check if the pdf is stored as a URL
   */
  isUrl(): boolean {
    return this.type === "url";
  }

  /**
   * Get the URL of the pdf if it's stored as a URL
   * @throws Error if the pdf is not stored as a URL
   */
  asUrl(): string {
    if (!this.isUrl()) {
      throw new Error("Pdf is not a URL");
    }
    return this.content;
  }

  /**
   * Get the base64 data and media type if the pdf is stored as base64
   * @returns [base64Data, mediaType]
   * @throws Error if the pdf is not stored as base64
   */
  asBase64(): [string, string] {
    if (this.type !== "base64") {
      throw new Error("Pdf is not base64");
    }
    return [this.content, this.mediaType || ""];
  }

  /**
   * Convert the pdf to a JSON representation
   */
  toJSON(): { url: string } | { base64: string; media_type: string } {
    if (this.type === "url") {
      return { url: this.content };
    }
    return {
      base64: this.content,
      media_type: this.mediaType || "",
    };
  }
}
