// PostHog runs on every page (instrumentation-client.ts) with autocapture on, so every click is
// already recorded with its text and link. `section` names a page region: autocapture also reads
// `data-ph-capture-attribute-*` from the clicked element's ancestors, so a click anywhere inside one
// carries `section`, and ./track.tsx uses the same name for `boundary_section_viewed`.
// Plain attributes, so it works in server components too: `<section {...section("hero")}>`.

export const SECTION_ATTR = 'data-ph-capture-attribute-section';

/** PostHog autocapture property naming the region a click came from. */
export function section(name: string) {
  return { [SECTION_ATTR]: name };
}
