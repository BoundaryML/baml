import { MinimalCodeSnippet } from './minimal-code-snippet';

const DEFAULT_BULLETS = [
  'First-class LLM functions',
  'Typed prompt boundaries',
  'Structured LLM outputs',
  'testset / test / assert.* blocks',
  'Expression-first style',
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
  headline = 'BAML is a statically-typed, expression-oriented language with first-class LLM functions.',
  lead = 'Define a class, write a Jinja-templated prompt, and get a typed result. BAML supplies typed prompt boundaries for structured-output LLM work in Python, TypeScript, and Ruby.',
  bullets = DEFAULT_BULLETS,
  code = DEFAULT_CODE,
}: MinimalHeroProps) {
  return (
    <article
      style={{
        color: '#1A1612',
        fontFamily: 'var(--font-sans), system-ui, sans-serif',
        maxWidth: 760,
      }}
    >
      <h1
        style={{
          fontSize: 'clamp(2.25rem, 4.5vw, 3.5rem)',
          fontWeight: 600,
          letterSpacing: '-0.025em',
          lineHeight: 1.06,
          margin: 0,
        }}
      >
        {headline}
      </h1>

      <p
        style={{
          color: '#2A2520',
          fontSize: 18,
          lineHeight: 1.55,
          marginTop: 32,
        }}
      >
        {lead}
      </p>

      <ul
        style={{
          color: '#1A1612',
          display: 'flex',
          flexDirection: 'column',
          fontSize: 17,
          gap: 14,
          listStyle: 'none',
          marginTop: 32,
          padding: 0,
        }}
      >
        {bullets.map((b) => (
          <li
            key={b}
            style={{
              alignItems: 'baseline',
              display: 'flex',
              gap: 12,
            }}
          >
            <span
              aria-hidden
              style={{ color: '#8A8580', fontVariantNumeric: 'tabular-nums' }}
            >
              -
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
