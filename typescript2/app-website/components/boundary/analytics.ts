// PostHog runs on every page (instrumentation-client.ts) with autocapture on, so every click is already
// recorded. `cta` labels a link or button so its autocaptured click carries which CTA it was and
// where on the page it sits: spread it onto the element, `<a {...cta("start_free", "hero")}>`.
// Plain attributes, so it works in server components too.

/** PostHog autocapture properties for a call to action: `cta` (what it is) and `location` (where). */
export function cta(name: string, location: string) {
  return {
    'data-ph-capture-attribute-cta': name,
    'data-ph-capture-attribute-location': location,
  };
}
