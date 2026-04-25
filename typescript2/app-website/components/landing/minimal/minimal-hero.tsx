import { MinimalCodeSnippet } from './minimal-code-snippet';

const DEFAULT_BULLETS = [
  'Incrementally adoptable',
  'Type-safety of TypeScript and Rust',
  'Type-safe exceptions',
  'LLMs as operators',
  'Really good agentic tooling',
];

const DEFAULT_CODE = `class Receipt {
  total float
  merchant string
  items string[]
}

function ExtractReceipt(text: string) -> Receipt {
  client GPT4
  prompt #"
    Extract the receipt fields from the text.
    {{ ctx.output_format }}
    {{ _.role("user") }} {{ text }}
  "#
}`;

interface MinimalHeroProps {
  headline?: string;
  lead?: string;
  bullets?: string[];
  code?: string;
}

export function MinimalHero({
  headline = 'BAML is a modern programming language built natively for LLMs.',
  lead = 'BAML runs in Python, TypeScript, and Ruby runtimes. It brings type-safety, structured outputs, and agentic tooling to every LLM call — so developers can build reliable AI applications.',
  bullets = DEFAULT_BULLETS,
  code = DEFAULT_CODE,
}: MinimalHeroProps) {
  return (
    <article
      style={{
        fontFamily: 'var(--font-sans), system-ui, sans-serif',
        color: '#1A1612',
        maxWidth: 760,
      }}
    >
      <h1
        style={{
          fontSize: 'clamp(2.25rem, 4.5vw, 3.5rem)',
          fontWeight: 600,
          lineHeight: 1.06,
          letterSpacing: '-0.025em',
          margin: 0,
        }}
      >
        {headline}
      </h1>

      <p
        style={{
          marginTop: 32,
          fontSize: 18,
          lineHeight: 1.55,
          color: '#2A2520',
        }}
      >
        {lead}
      </p>

      <ul
        style={{
          marginTop: 32,
          padding: 0,
          listStyle: 'none',
          display: 'flex',
          flexDirection: 'column',
          gap: 14,
          fontSize: 17,
          color: '#1A1612',
        }}
      >
        {bullets.map((b) => (
          <li
            key={b}
            style={{
              display: 'flex',
              alignItems: 'baseline',
              gap: 12,
            }}
          >
            <span
              aria-hidden
              style={{ color: '#8A8580', fontVariantNumeric: 'tabular-nums' }}
            >
              —
            </span>
            <span>{b}</span>
          </li>
        ))}
      </ul>

      <div style={{ marginTop: 40 }}>
        <MinimalCodeSnippet code={code} lang="baml" />
      </div>
    </article>
  );
}
