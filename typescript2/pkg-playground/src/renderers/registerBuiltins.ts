/**
 * Registers built-in result renderers (e.g. baml.http.Request → curl).
 * Import this once so default renderers are available.
 */

import { registerResultRenderer } from '../result-renderers';
import { HttpRequestCurlRenderer } from './HttpRequestCurl';

export function registerBuiltinResultRenderers(): void {
  registerResultRenderer('baml.http.Request', HttpRequestCurlRenderer);
}
