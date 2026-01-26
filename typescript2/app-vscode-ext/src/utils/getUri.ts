import { Uri, type Webview } from 'vscode';

/**
 * A helper function which will get the webview URI of a given file or resource.
 */
export function getUri(
  webview: Webview,
  extensionUri: Uri,
  pathList: string[]
): Uri {
  return webview.asWebviewUri(Uri.joinPath(extensionUri, ...pathList));
}
