/**
 * Per-session token minted by `baml playground` (browser mode) and carried on
 * the page URL as `?token=...`. Every same-origin /api request — fetch and
 * WebSocket alike — must present it back to the server; browsers can't set
 * custom WS headers, so it rides the query string.
 *
 * In VS Code webviews (no token minted) `getPlaygroundToken` returns null and
 * `withPlaygroundToken` is a no-op.
 */

/** Session token from the current page URL, or null when absent. */
export function getPlaygroundToken(): string | null {
  return new URLSearchParams(window.location.search).get('token');
}

/** Append the session token (if any) to a same-origin API/WS URL. */
export function withPlaygroundToken(url: string): string {
  const token = getPlaygroundToken();
  if (!token) return url;
  return `${url}${url.includes('?') ? '&' : '?'}token=${encodeURIComponent(token)}`;
}
