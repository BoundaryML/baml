import { vscode } from '../../vscode';

export const findMediaFile = async (path: string): Promise<Uint8Array> => {
  // Try to get the URI from the backend
  const resp = await vscode.readLocalFile('', path);
  if (resp.uri) {
    // Fetch the file from the URI if provided
    const res = await fetch(resp.uri);
    if (!res.ok) {
      throw new Error(`Failed to fetch file from URI: ${resp.uri}`);
    }
    const buffer = await res.arrayBuffer();
    return new Uint8Array(buffer);
  }
  if (resp.readError) {
    throw new Error(`Failed to read file: ${path}\n${resp.readError}`);
  }
  if (resp.contents) {
    // Fallback: decode base64 contents
    const contents = resp.contents;
    // Use atob to decode base64 to binary string, then to Uint8Array
    const binary = atob(contents);
    const len = binary.length;
    const bytes = new Uint8Array(len);
    for (let i = 0; i < len; i++) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  }
  throw new Error(`Unknown error: '${path}'`);
};
