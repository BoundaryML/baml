// Create a custom client-side implementation that matches the BamlImage interface
// This will be serialized and sent to the server which will handle it appropriately
export type ImageLike =
  | {
      url: string;
    }
  | {
      base64: string;
      media_type?: string;
    };

// Define an enum for image source types
export type ImageSourceType = 'file' | 'url';

// Helper functions to create image objects in the format expected by the server
export const ClientImage = {
  fromUrl: (url: string): ImageLike => ({
    url,
  }),

  fromBase64: (mediaType: string, base64: string): ImageLike => ({
    media_type: mediaType,
    base64,
  }),
};

// Helper function to convert files to base64
export const convertFileToBase64 = (file: File): Promise<string> => {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.readAsDataURL(file);
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = (error) => reject(error);
  });
};

// Validate if a URL is valid (starts with http:// or https://)
export const isValidImageUrl = (url: string): boolean => {
  return !!url && (url.startsWith('http://') || url.startsWith('https://'));
};
