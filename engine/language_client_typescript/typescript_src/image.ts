/**
 * Browser-compatible implementation of BamlImage
 */
export class BamlImage {
  private constructor(
    private readonly type: 'url' | 'base64',
    private readonly content: string,
    private readonly mediaType?: string,
  ) {}

  /**
   * Create a BamlImage from a URL
   */
  static fromUrl(url: string, mediaType?: string): BamlImage {
    return new BamlImage('url', url, mediaType);
  }

  /**
   * Create a BamlImage from base64 encoded data
   */
  static fromBase64(mediaType: string, base64: string): BamlImage {
    return new BamlImage('base64', base64, mediaType);
  }

  /**
   * Create a BamlImage from a File object
   */
  static async fromFile(file: File): Promise<BamlImage> {
    return BamlImage.fromBlob(file, file.type);
  }

  /**
   * Create a BamlImage from a Blob object
   */
  static async fromBlob(blob: Blob, mediaType?: string): Promise<BamlImage> {
    const base64 = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result as string);
      reader.onerror = reject;
      reader.readAsDataURL(blob);
    });
    // Remove the data URL prefix to get just the base64 string
    const base64Data = base64.replace(/^data:.*?;base64,/, '');
    return BamlImage.fromBase64(mediaType || blob.type, base64Data);
  }

  /**
   * Create a BamlImage by fetching from a URL
   */
  static async fromUrlAsync(url: string): Promise<BamlImage> {
    const response = await fetch(url);
    const blob = await response.blob();
    return BamlImage.fromBlob(blob);
  }

  /**
   * Check if the image is stored as a URL
   */
  isUrl(): boolean {
    return this.type === 'url';
  }

  /**
   * Get the URL of the image if it's stored as a URL
   * @throws Error if the image is not stored as a URL
   */
  asUrl(): string {
    if (!this.isUrl()) {
      throw new Error('Image is not a URL');
    }
    return this.content;
  }

  /**
   * Get the base64 data and media type if the image is stored as base64
   * @returns [base64Data, mediaType]
   * @throws Error if the image is not stored as base64
   */
  asBase64(): [string, string] {
    if (this.type !== 'base64') {
      throw new Error('Image is not base64');
    }
    return [this.content, this.mediaType || ''];
  }

  /**
   * Convert the image to a JSON representation
   */
  toJSON(): { url: string } | { base64: string; media_type: string } {
    if (this.type === 'url') {
      return { url: this.content };
    }
    return {
      base64: this.content,
      media_type: this.mediaType || '',
    };
  }
}
