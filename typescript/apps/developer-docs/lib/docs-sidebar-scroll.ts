export const DOCS_SIDEBAR_SCROLL_STORAGE_KEY = "boundary-docs-sidebar-scroll"

export const DOCS_SIDEBAR_SCROLL_RESTORE_SCRIPT = `
try {
  var saved = sessionStorage.getItem('${DOCS_SIDEBAR_SCROLL_STORAGE_KEY}');
  if (saved) {
    var state = JSON.parse(saved);
    window.__BOUNDARY_DOCS_SIDEBAR_SCROLL__ = state;
  }
} catch (_) {}
`

declare global {
  interface Window {
    __BOUNDARY_DOCS_SIDEBAR_SCROLL__?: {
      pathname: string
      scrollTop: number
    }
  }
}
