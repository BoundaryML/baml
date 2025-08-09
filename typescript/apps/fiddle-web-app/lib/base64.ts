// Utility functions for base64 encoding/decoding that work in both client and server environments

export function encodeBase64(str: string): string {
  if (typeof window !== 'undefined') {
    // Client-side
    return btoa(str);
  } else {
    // Server-side
    return Buffer.from(str).toString('base64');
  }
}

export function decodeBase64(str: string): string {
  if (typeof window !== 'undefined') {
    // Client-side
    return atob(str);
  } else {
    // Server-side
    return Buffer.from(str, 'base64').toString('utf-8');
  }
}