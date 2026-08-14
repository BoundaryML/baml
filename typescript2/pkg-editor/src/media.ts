/**
 * Media-file helpers shared by the workbench and editor backends.
 *
 * Text files (.baml, .toml, .json) are stored as raw content strings.
 * Media files (.png, .jpg, etc.) are stored as data-URL strings.
 */

export const MIME_TYPES: Record<string, string> = {
  png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif',
  svg: 'image/svg+xml', webp: 'image/webp', ico: 'image/x-icon', bmp: 'image/bmp',
  mp3: 'audio/mpeg', wav: 'audio/wav', ogg: 'audio/ogg',
  mp4: 'video/mp4', webm: 'video/webm',
  pdf: 'application/pdf',
};

export function mimeFromPath(path: string): string {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  return MIME_TYPES[ext] ?? 'application/octet-stream';
}

export function isMediaPath(filename: string): boolean {
  const ext = filename.split('.').pop()?.toLowerCase() ?? '';
  return ext in MIME_TYPES;
}

/** Encode binary data as a data URL. */
export function toDataUrl(data: Uint8Array, mime: string): string {
  let binary = '';
  for (const byte of data) binary += String.fromCharCode(byte);
  return `data:${mime};base64,${btoa(binary)}`;
}

/** Decode a data URL back to binary. */
export function fromDataUrl(dataUrl: string): Uint8Array {
  const base64 = dataUrl.split(',')[1] ?? '';
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}
