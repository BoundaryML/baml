const BG = '#FBF7ED';
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
  {
    label: 'Error-correcting parser',
    syntax: 'function Extract(img: Image) -> Receipt',
  },
  {
    label: 'Tagged union match dispatch',
    syntax: 'match (tool) { r: ReadFile => read(r.path) }',
  },
  {
    label: 'Typed errors and retries',
    syntax: 'function Fetch() -> Result throws Timeout',
  },
  {
    label: 'Generics, lambdas, namespaces',
    syntax: 'function Apply<T>(f: (x: T) -> T, x: T) -> T',
  },
];

const RIGHT = [
  {
    label: 'Better context',
    syntax: 'baml-cli describe',
  },
  {
    label: 'Streaming with typed partials',
    syntax: 'stream Extract(img) -> partial Receipt',
  },
  {
    label: 'One interface, every LLM provider',
    syntax: 'client "openai/gpt-4o"',
  },
  {
    label: 'Tests live next to your prompts',
    syntax: 'test Happy { functions [Classify] }',
  },
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
          .why-item { line-height: 1.35 !important; font-size: 15px !important; }
          .why-item code { white-space: normal !important; }
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
              fontFamily:
                'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
              fontSize: 'clamp(5px, 0.78vw, 12px)',
              fontWeight: 600,
              lineHeight: 1,
              margin: '0 0 28px',
              maxWidth: '100%',
              overflow: 'hidden',
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
            <span style={{ color: ACCENT, fontWeight: 500 }}>Rust</span> and
            used by some of the world&apos;s largest companies. It has a
            compiler, VM, LSP, formatter, type system (with inferred error
            types), and drops into{' '}
            <span style={{ fontWeight: 500 }}>Python</span>,{' '}
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

function Column({ items }: { items: { label: string; syntax: string }[] }) {
  return (
    <ul style={{ listStyle: 'none', padding: 0, margin: 0 }}>
      {items.map((item) => (
        <li
          key={item.label}
          className="why-item"
          style={{
            fontSize: '17px',
            fontWeight: 400,
            color: INK,
            lineHeight: 1.35,
            marginBottom: 26,
          }}
        >
          <span>{item.label}</span>
          <code
            style={{
              color: '#6D28D9',
              display: 'block',
              fontSize: '11px',
              lineHeight: 1.35,
              marginTop: 6,
              opacity: 0.72,
              whiteSpace: 'nowrap',
            }}
          >
            {item.syntax}
          </code>
        </li>
      ))}
    </ul>
  );
}
