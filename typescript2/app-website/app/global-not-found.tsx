import type { Metadata } from 'next';

import './globals.css';

export const metadata: Metadata = {
  title: '404 — Lost sheep | BAML',
};

// `experimental.globalNotFound` is enabled, so this renders the FULL document
// for unmatched routes (it does not use the root layout). Kept self-contained
// and on-theme with the site's cream/editorial palette.
export default function GlobalNotFound() {
  return (
    <html lang="en">
      <body
        style={{
          alignItems: 'center',
          background: '#FBF7ED',
          color: '#1A1612',
          display: 'flex',
          fontFamily:
            "'Instrument Serif', ui-serif, Georgia, 'Times New Roman', serif",
          justifyContent: 'center',
          margin: 0,
          minHeight: '100vh',
        }}
      >
        <main
          style={{
            maxWidth: 560,
            padding: '24px',
            textAlign: 'center',
          }}
        >
          <div
            aria-hidden
            style={{ fontSize: 72, lineHeight: 1, marginBottom: 12 }}
          >
            🐑
          </div>
          <p
            style={{
              color: '#8A8580',
              fontFamily:
                'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
              fontSize: 12,
              letterSpacing: '0.18em',
              margin: '0 0 20px',
            }}
          >
            404
          </p>
          <h1
            style={{
              fontSize: 'clamp(28px, 5vw, 42px)',
              fontWeight: 400,
              letterSpacing: '-0.01em',
              lineHeight: 1.2,
              margin: '0 0 32px',
            }}
          >
            Uh oh! Looks like you wandered from the flock.
          </h1>
          <a
            href="/"
            style={{
              background: '#1A1612',
              borderRadius: 10,
              color: '#fff',
              display: 'inline-block',
              fontFamily: 'ui-sans-serif, system-ui, sans-serif',
              fontSize: 15,
              fontWeight: 500,
              padding: '12px 24px',
              textDecoration: 'none',
            }}
          >
            Back to the pasture
          </a>
        </main>
      </body>
    </html>
  );
}
