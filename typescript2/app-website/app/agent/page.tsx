import Link from 'next/link';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { readAgentMarkdown } from '@/lib/agent-content';

export const metadata = {
  description: 'Plain-text agent-readable view of boundaryml.com',
  title: 'BAML: agent mode',
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
        background: BG,
        color: INK,
        fontFamily: MONO,
        minHeight: '100vh',
        padding: '48px 24px 96px',
      }}
    >
      <div style={{ margin: '0 auto', maxWidth: 720 }}>
        <div
          style={{
            alignItems: 'center',
            color: '#5C5852',
            display: 'flex',
            fontSize: 12,
            justifyContent: 'space-between',
            letterSpacing: '0.08em',
            marginBottom: 32,
            textTransform: 'uppercase',
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
            borderTop: '1px solid #D9D3C4',
            color: '#8A8580',
            fontSize: 11,
            letterSpacing: '0.08em',
            marginTop: 48,
            paddingTop: 16,
            textTransform: 'uppercase',
          }}
        >
          raw:{' '}
          <a href="/agent.md" style={{ color: ACCENT }}>
            /agent.md
          </a>{' '}
          ·{' '}
          <a href="/llms.txt" style={{ color: ACCENT }}>
            /llms.txt
          </a>
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
