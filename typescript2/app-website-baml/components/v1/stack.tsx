import { section } from '../analytics';
import Reveal from '../reveal';
import { Cmd, LANES, LEFT, PAD_Y, WRAP } from '../ui';

// Lists follow Boundary's stack diagram (Oct 2026).
const ROWS: { label: string; value: string; lead?: boolean }[] = [
  {
    label: 'Languages, via baml generate',
    lead: true,
    value: 'Python, TypeScript, Go, Rust, Java, Swift and C++',
  },
  {
    label: 'AI integrations',
    value:
      'OpenAI, Anthropic, Gemini, Vertex, Bedrock, Azure, Ollama, OpenRouter, Vercel, Claude Code, MCP tools, and anything compatible',
  },
  {
    label: 'Agent interfaces',
    value:
      'The CLI, the LSP, the agent skill, the playground, VS Code and Cursor',
  },
];

export default function Stack() {
  return (
    <section
      {...section('stack')}
      aria-labelledby="stack-title"
      className={`${WRAP} ${PAD_Y} ${LANES}`}
    >
      <div className={`${LEFT} flex flex-col gap-6`}>
        <h2 className="h2 lg:max-w-[450px]" id="stack-title">
          Write it once. Run it in your stack.
        </h2>
        <p className="text-[17px] leading-[27px] text-body">
          <Cmd>baml generate</Cmd> turns your BAML into a typed client for the
          language you already ship, and it works with the models and agent
          tools you already use.
        </p>
        <p className="text-sm leading-5 text-muted">
          Runs standalone on macOS, Linux and Windows. C# is coming soon.
        </p>
      </div>
      <Reveal className="flex-1 min-w-0">
        <dl>
          {ROWS.map((r, i) => (
            <div
              className={`flex flex-col sm:flex-row gap-3 sm:gap-8 pt-7 pb-8 border-t ${i === 0 ? 'border-rule-accent' : 'border-rule'} last:border-b last:border-b-rule`}
              key={r.label}
            >
              <dt
                className={`sm:w-[150px] shrink-0 ${r.lead ? 'sm:pt-2' : 'sm:pt-1'} text-sm leading-5 text-muted`}
              >
                {r.label}
              </dt>
              <dd
                className={
                  r.lead
                    ? 'font-display font-medium text-[clamp(26px,2.7vw,38px)] leading-[1.26] tracking-[-0.03em] text-balance'
                    : 'font-display text-[clamp(19px,1.8vw,26px)] leading-[1.38] tracking-[-0.02em] text-ink-2 text-pretty'
                }
              >
                {r.value}
              </dd>
            </div>
          ))}
        </dl>
      </Reveal>
    </section>
  );
}
