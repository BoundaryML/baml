import { MinimalHero } from './minimal-hero';
import { MinimalNav } from './minimal-nav';
import { MinimalSidebar } from './minimal-sidebar';

/**
 * Reference assembly of the minimal landing components.
 *
 * Drop this into a page to see the full layout, or compose the parts
 * (`MinimalNav`, `MinimalHero`, `MinimalSidebar`) directly for finer control.
 */
export function MinimalLanding() {
  return (
    <div
      style={{
        minHeight: '100vh',
        background: '#FBFAF5',
        color: '#1A1612',
      }}
    >
      <MinimalNav />
      <main
        style={{
          maxWidth: 1200,
          margin: '0 auto',
          padding: '72px 48px 120px',
          display: 'grid',
          gridTemplateColumns: 'minmax(0, 1fr) 320px',
          columnGap: 96,
          alignItems: 'start',
        }}
      >
        <MinimalHero />
        <MinimalSidebar />
      </main>
    </div>
  );
}
