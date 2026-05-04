const BG = '#ffffff';
const INK = '#1A1612';
const BORDER = '#D9D3C4';
const ACCENT = '#6D28D9';
const DASH = 'rgba(109, 40, 217, 0.35)';

const BAML_ASCII = String.raw`          _____                    _____                    _____                    _____
         /\    \                  /\    \                  /\    \                  /\    \
        /::\    \                /::\    \                /::\____\                /::\____\
       /::::\    \              /::::\    \              /::::|   |               /:::/    /
      /::::::\    \            /::::::\    \            /:::::|   |              /:::/    /
     /:::/\:::\    \          /:::/\:::\    \          /::::::|   |             /:::/    /
    /:::/__\:::\    \        /:::/__\:::\    \        /:::/|::|   |            /:::/    /
   /::::\   \:::\    \      /::::\   \:::\    \      /:::/ |::|   |           /:::/    /
  /::::::\   \:::\    \    /::::::\   \:::\    \    /:::/  |::|___|______    /:::/    /
 /:::/\:::\   \:::\ ___\  /:::/\:::\   \:::\    \  /:::/   |::::::::\    \  /:::/    /
/:::/__\:::\   \:::|    |/:::/  \:::\   \:::\____\/:::/    |:::::::::\____\/:::/____/
\:::\   \:::\  /:::|____|\::/    \:::\  /:::/    /\::/    / ~~~~~/:::/    /\:::\    \
 \:::\   \:::\/:::/    /  \/____/ \:::\/:::/    /  \/____/      /:::/    /  \:::\    \
  \:::\   \::::::/    /            \::::::/    /               /:::/    /    \:::\    \
   \:::\   \::::/    /              \::::/    /               /:::/    /      \:::\    \
    \:::\  /:::/    /               /:::/    /               /:::/    /        \:::\    \
     \:::\/:::/    /               /:::/    /               /:::/    /          \:::\    \
      \::::::/    /               /:::/    /               /:::/    /            \:::\    \
       \::::/    /               /:::/    /               /:::/    /              \:::\____\
        \::/____/                \::/    /                \::/    /                \::/    /
         ~~                       \/____/                  \/____/                  \/____/`;

const LEFT = [
  'Schema aware parsing',
  'Tagged union match dispatch',
  'Typed errors and retries',
  'Generics, lambdas, namespaces',
];

const RIGHT = [
  'Better context',
  'Streaming with typed partials',
  'One interface, every LLM provider',
  'Tests live next to your prompts',
];

export function WhyALanguage() {
  return (
    <section
      className="w-full"
      aria-label="BAML"
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
          borderBottom: `1px solid ${BORDER}`,
          padding: '99px 48px 132px',
        }}
      >
        <div className="mx-auto flex w-full max-w-[900px] flex-col items-center text-center">
          <pre
            aria-label="BAML"
            style={{
              color: ACCENT,
              fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
              fontSize: 'clamp(5px, 0.78vw, 12px)',
              fontWeight: 600,
              lineHeight: 1,
              margin: '0 0 28px',
              maxWidth: '100%',
              overflowX: 'auto',
              textAlign: 'left',
              whiteSpace: 'pre',
            }}
          >
            {BAML_ASCII}
          </pre>

          <div
            aria-hidden="true"
            style={{
              width: 56,
              borderTop: `1px dashed ${DASH}`,
              margin: '0 0 28px',
            }}
          />

          <p
            style={{
              fontSize: 'clamp(17px, 1.6vw, 20px)',
              fontWeight: 400,
              lineHeight: 1.55,
              letterSpacing: '-0.005em',
              color: INK,
              maxWidth: 720,
              margin: '0 0 88px',
            }}
          >
            BAML is a programming language, built in{' '}
            <span style={{ color: ACCENT, fontWeight: 500 }}>Rust</span>{' '}
            and used by some of the world&apos;s largest companies. It has a
            compiler, VM, LSP, formatter, type system (with inferred error
            types), and drops into <span style={{ fontWeight: 500 }}>Python</span>,{' '}
            <span style={{ fontWeight: 500 }}>TypeScript</span>,{' '}
            <span style={{ fontWeight: 500 }}>Go</span>, and the{' '}
            <span style={{ fontWeight: 500 }}>browser</span> so teams can adopt
            it incrementally without rewriting their stack.
          </p>

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
