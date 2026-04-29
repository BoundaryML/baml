const BG = '#ffffff';
const INK = '#1A1612';
const MUTED = '#5C5852';
const KICKER = '#8A8580';
const BORDER = '#D9D3C4';
const ACCENT = '#6D28D9';
const DASH = 'rgba(109, 40, 217, 0.35)';

const LEFT = [
  'schema-aware prompts',
  'type safety',
  'multi-language codegen',
  'retries & fallbacks',
];

const RIGHT = [
  'schema repair',
  'streaming',
  'tests & playground',
  'LSP & autocomplete',
];

export function WhyALanguage() {
  return (
    <section
      className="w-full"
      aria-labelledby="why-a-language-heading"
      style={{ backgroundColor: BG }}
    >
      <style>{`
        @media (max-width: 900px) {
          .why-cols { grid-template-columns: 1fr !important; max-width: 420px !important; }
          .why-divider { display: none !important; }
          .why-item { line-height: 2.2em !important; font-size: 15px !important; }
        }
      `}</style>

      <div
        className="mx-auto"
        style={{
          maxWidth: '1600px',
          borderLeft: `1px solid ${BORDER}`,
          borderRight: `1px solid ${BORDER}`,
          borderBottom: `1px solid ${BORDER}`,
          padding: '40px 48px 120px',
        }}
      >
        <div className="mx-auto flex w-full max-w-[900px] flex-col items-center text-center">
          <p
            style={{
              fontSize: '13px',
              fontWeight: 500,
              letterSpacing: '0.12em',
              textTransform: 'uppercase',
              color: KICKER,
              margin: '0 0 20px',
            }}
          >
            Why a language
          </p>

          <div
            aria-hidden="true"
            style={{
              width: 56,
              borderTop: `1px dashed ${DASH}`,
              margin: '0 0 28px',
            }}
          />

          <h2
            id="why-a-language-heading"
            style={{
              fontSize: 'clamp(2rem, 4.5vw, 3.5rem)',
              fontWeight: 600,
              lineHeight: 1.02,
              letterSpacing: '-0.03em',
              color: INK,
              margin: '0 0 88px',
            }}
          >
            Every guarantee, built in
            <span style={{ color: ACCENT }}>.</span>
          </h2>

          <div
            className="why-cols"
            style={{
              display: 'grid',
              gridTemplateColumns: '1fr auto 1fr',
              columnGap: '48px',
              width: '100%',
              maxWidth: '800px',
              textAlign: 'center',
              alignItems: 'stretch',
              justifyItems: 'center',
            }}
          >
            <Column items={LEFT} />
            <div
              aria-hidden="true"
              className="why-divider"
              style={{ borderLeft: `1px dashed ${DASH}`, width: 0 }}
            />
            <Column items={RIGHT} />
          </div>

          <div
            aria-hidden="true"
            style={{
              width: 56,
              borderTop: `1px dashed ${DASH}`,
              margin: '72px 0 20px',
            }}
          />

          <p
            style={{
              fontFamily: 'var(--font-serif)',
              fontStyle: 'italic',
              fontSize: '18px',
              color: MUTED,
              margin: 0,
            }}
          >
            Engineering, not magic.
          </p>
        </div>
      </div>
    </section>
  );
}

function Column({ items }: { items: string[] }) {
  return (
    <ul style={{ listStyle: 'none', padding: 0, margin: 0 }}>
      {items.map((item) => (
        <li
          key={item}
          className="why-item"
          style={{
            fontSize: '17px',
            fontWeight: 400,
            color: INK,
            lineHeight: '2.5em',
          }}
        >
          {item}
        </li>
      ))}
    </ul>
  );
}
