import Link from 'next/link';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { readAgentMarkdown } from '@/lib/agent-content';

export const metadata = {
  title: 'BAML — agent mode',
  description: 'Plain-text agent-readable view of boundaryml.com',
};

const MONO =
  'ui-monospace, SFMono-Regular, "JetBrains Mono", Menlo, Consolas, monospace';
const ACCENT = '#7C3AED';
const BG = '#ffffff';
const INK = '#1A1612';

export default async function AgentPage() {
  const markdown = await readAgentMarkdown();

  return (
    <div
      style={{
        minHeight: '100vh',
        background: BG,
        color: INK,
        fontFamily: MONO,
        padding: '48px 24px 96px',
      }}
    >
      <div style={{ maxWidth: 720, margin: '0 auto' }}>
        <div
          style={{
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            marginBottom: 32,
            fontSize: 12,
            letterSpacing: '0.08em',
            textTransform: 'uppercase',
            color: '#5C5852',
          }}
        >
          <span>agent mode</span>
          <Link
            href="/?from=toggle"
            prefetch
            style={{ color: ACCENT, textDecoration: 'none' }}
          >
            ← back to human mode
          </Link>
        </div>

        <article
          className="agent-markdown"
          style={{ fontSize: 14, lineHeight: 1.7 }}
        >
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{markdown}</ReactMarkdown>
        </article>

        <div
          style={{
            marginTop: 48,
            paddingTop: 16,
            borderTop: '1px solid #D9D3C4',
            fontSize: 11,
            letterSpacing: '0.08em',
            textTransform: 'uppercase',
            color: '#8A8580',
          }}
        >
          raw: <a href="/agent.md" style={{ color: ACCENT }}>/agent.md</a> ·
          {' '}
          <a href="/llms.txt" style={{ color: ACCENT }}>/llms.txt</a>
        </div>
      </div>

      <style>{`
        .agent-markdown h1,
        .agent-markdown h2,
        .agent-markdown h3 {
          color: ${ACCENT};
          font-family: ${MONO};
          letter-spacing: -0.01em;
        }
        .agent-markdown h1 { font-size: 22px; margin: 0 0 16px; }
        .agent-markdown h2 { font-size: 16px; margin: 28px 0 12px; text-transform: uppercase; letter-spacing: 0.06em; }
        .agent-markdown h3 { font-size: 14px; margin: 20px 0 8px; }
        .agent-markdown p { margin: 0 0 12px; }
        .agent-markdown ul, .agent-markdown ol { margin: 0 0 12px; padding-left: 20px; }
        .agent-markdown li { margin: 4px 0; }
        .agent-markdown a { color: ${ACCENT}; text-decoration: underline; text-underline-offset: 2px; }
        .agent-markdown code {
          background: #EDE6D6;
          padding: 1px 5px;
          border-radius: 3px;
          font-size: 0.9em;
        }
        .agent-markdown strong { color: ${INK}; }
      `}</style>
    </div>
  );
}
