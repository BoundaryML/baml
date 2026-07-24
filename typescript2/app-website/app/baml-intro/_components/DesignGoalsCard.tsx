/* A github.com "embed" — reproduces the dark-mode chrome of the
 * Microsoft/TypeScript "TypeScript Design Goals" wiki page (global nav with the
 * Octocat mark, the repo tab bar with the active Wiki tab, the page title and
 * "edited this page" byline) the way a site embeds a tweet. It renders only the
 * Non-Goals list and highlights the one about a sound type system.
 *
 * Self-contained: GitHub's palette is fixed regardless of the surrounding page
 * theme, so the card carries its own dark colors inline rather than inheriting
 * the article's CSS variables. */

const WIKI_URL =
  'https://github.com/Microsoft/TypeScript/wiki/TypeScript-Design-Goals';

// GitHub dark palette
const C = {
  navBg: '#010409',
  pageBg: '#0d1117',
  border: '#3d444d',
  text: '#f0f6fc',
  body: '#d1d7e0',
  muted: '#9198a1',
  link: '#4493f8',
  markBg: 'rgba(187,128,9,0.18)',
  markEdge: '#bb8009',
};

const SANS =
  '-apple-system, BlinkMacSystemFont, "Segoe UI", "Noto Sans", Helvetica, Arial, sans-serif';

// GitHub's "Non-Goals" list from the TypeScript Design Goals wiki. `soundness`
// flags the one we highlight (a sound / "provably correct" type system).
const NON_GOALS: { text: string; soundness?: boolean }[] = [
  {
    text:
      'Exactly mimic the design of existing languages. Instead, use the behavior of JavaScript and the intentions of program authors as a guide for what makes the most sense in the language.',
  },
  {
    text:
      'Aggressively optimize the runtime performance of programs. Instead, emit idiomatic JavaScript code that plays well with the performance characteristics of runtime platforms.',
  },
  {
    // SOUNDNESS_INDEX points here — the only goal the card renders.
    soundness: true,
    text:
      'Apply a sound or "provably correct" type system. Instead, strike a balance between correctness and productivity.',
  },
  {
    text:
      'Provide an end-to-end build pipeline. Instead, make the system extensible so that external tools can use the compiler for more complex build workflows.',
  },
  {
    text:
      'Add or rely on run-time type information in programs, or emit different code based on the results of the type system. Instead, encourage programming patterns that do not require run-time metadata.',
  },
  {
    text:
      'Provide additional runtime functionality or libraries. Instead, use TypeScript to describe existing libraries.',
  },
  {
    text:
      'Introduce behaviour that is likely to surprise users. Instead have due consideration for patterns adopted by other commonly-used languages.',
  },
];

const SOUNDNESS_INDEX = NON_GOALS.findIndex((g) => g.soundness);

function Octocat() {
  return (
    <svg aria-hidden height={32} viewBox="0 0 16 16" width={32}>
      <path
        d="M8 0c4.42 0 8 3.58 8 8a8.013 8.013 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.38A7.995 7.995 0 0 1 0 8c0-4.42 3.58-8 8-8Z"
        fill={C.text}
      />
    </svg>
  );
}

function Hamburger() {
  return (
    <svg aria-hidden height={16} viewBox="0 0 16 16" width={16}>
      <path
        d="M1 2.75A.75.75 0 0 1 1.75 2h12.5a.75.75 0 0 1 0 1.5H1.75A.75.75 0 0 1 1 2.75Zm0 5A.75.75 0 0 1 1.75 7h12.5a.75.75 0 0 1 0 1.5H1.75A.75.75 0 0 1 1 7.75ZM1.75 12h12.5a.75.75 0 0 1 0 1.5H1.75a.75.75 0 0 1 0-1.5Z"
        fill={C.muted}
      />
    </svg>
  );
}

export function DesignGoalsCard() {
  return (
    <figure style={{ margin: '2rem 0' }}>
      <a
        href={WIKI_URL}
        rel="noopener noreferrer"
      style={{
        background: C.pageBg,
        border: `1px solid ${C.border}`,
        borderRadius: 8,
        color: C.body,
        display: 'block',
        fontFamily: SANS,
        maxWidth: '100%',
        overflow: 'hidden',
        textDecoration: 'none',
        width: '100%',
      }}
      target="_blank"
    >
      {/* global nav bar */}
      <div
        style={{
          alignItems: 'center',
          background: C.navBg,
          borderBottom: `1px solid ${C.border}`,
          display: 'flex',
          gap: 12,
          padding: '12px 16px',
        }}
      >
        <Hamburger />
        <Octocat />
        <span style={{ alignItems: 'center', display: 'flex', fontSize: 14, minWidth: 0 }}>
          <span style={{ color: C.text }}>microsoft</span>
          <span style={{ color: C.muted, padding: '0 6px' }}>/</span>
          <span style={{ color: C.text, fontWeight: 600 }}>TypeScript</span>
        </span>
      </div>

      {/* page content */}
      <div style={{ padding: '24px 20px' }}>
        <div
          style={{
            color: C.text,
            fontSize: 28,
            fontWeight: 400,
            lineHeight: 1.25,
          }}
        >
          TypeScript Design Non-Goals
        </div>
        <div style={{ color: C.muted, fontSize: 14, marginTop: 8 }}>
          Orta edited this page on Feb 26, 2020 ·{' '}
          <span style={{ color: C.link }}>5 revisions</span>
        </div>

        <hr
          style={{
            border: 0,
            borderTop: `1px solid ${C.border}`,
            margin: '20px 0',
          }}
        />

        <ol
          style={{
            color: C.body,
            fontSize: 16,
            lineHeight: 1.6,
            listStyle: 'none',
            margin: 0,
            padding: 0,
          }}
        >
          {/* Only the one we're calling out — #3, the soundness non-goal. */}
          <li
            style={{
              background: C.markBg,
              borderLeft: `3px solid ${C.markEdge}`,
              borderRadius: '0 6px 6px 0',
              color: C.text,
              display: 'flex',
              gap: 10,
              lineHeight: 1.6,
              margin: '8px -10px',
              padding: '10px',
            }}
          >
            <span
              style={{
                color: C.muted,
                flex: 'none',
                fontVariantNumeric: 'tabular-nums',
                minWidth: '1.6em',
                textAlign: 'right',
              }}
            >
              {SOUNDNESS_INDEX + 1}.
            </span>
            <span>{NON_GOALS[SOUNDNESS_INDEX].text}</span>
          </li>
        </ol>
      </div>
      </a>
      <figcaption
        style={{
          color: 'var(--l6-muted)',
          fontSize: '14px',
          marginTop: '0.6rem',
        }}
      >
        {'Source: '}
        <a
          className="l6-link"
          href={WIKI_URL}
          rel="noopener noreferrer"
          target="_blank"
        >
          TypeScript Design Goals — Microsoft/TypeScript Wiki
        </a>
      </figcaption>
    </figure>
  );
}
