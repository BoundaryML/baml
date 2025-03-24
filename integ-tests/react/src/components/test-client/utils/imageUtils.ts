// Define an enum for image source types
export type ImageSourceType = 'file' | 'url'

// Validate if a URL is valid (starts with http:// or https://)
export const isValidImageUrl = (url: string): boolean => {
  return !!url && (url.startsWith('http://') || url.startsWith('https://'))
}
