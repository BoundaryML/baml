import { vscode } from '../../vscode'

export const findMediaFile = async (path: string): Promise<Uint8Array> => {
  // VSCode can read files directly
  if (vscode.isVscode()) {
    return await vscode.readFile(path)
  }

  // For Zed/non-VSCode editors, get the URI and fetch via HTTP
  try {
    console.log('Non-VSCode mode: fetching file via URI for path:', path);
    const uri = await vscode.asWebviewUri('', path);
    console.log('Got URI:', uri);
    
    const response = await fetch(uri);
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }
    
    const arrayBuffer = await response.arrayBuffer();
    return new Uint8Array(arrayBuffer);
  } catch (error) {
    console.error('Failed to fetch file via URI:', error);
    throw new Error(`Failed to load media file '${path}': ${error instanceof Error ? error.message : String(error)}`);
  }
}
