import { section } from '../analytics';
import { Cmd } from '../ui';

const ROWS: { label: string; items: string[] }[] = [
  {
    items: ['Python', 'TypeScript', 'Go', 'Rust', 'Java', 'Swift', 'C++'],
    label: 'Languages, via baml generate',
  },
  {
    items: [
      'OpenAI',
      'Anthropic',
      'Gemini',
      'Vertex',
      'Bedrock',
      'Azure',
      'Ollama',
      'OpenRouter',
      'Vercel',
      'Claude Code',
      'MCP tools',
      'anything compatible',
    ],
    label: 'AI integrations',
  },
];

export default function Stack() {
  return (
    <section
      {...section('stack')}
      aria-labelledby="stack-title"
      className="max-w-[1440px] mx-auto px-5 md:px-12 py-20 md:py-36 scroll-mt-16"
    >
      <h2 className="v2-h2 max-w-250" id="stack-title">
        Write it once. Run anywhere.
      </h2>
      <p className="max-w-160 mt-6 text-xl leading-[1.5] text-white/70">
        <Cmd>baml generate</Cmd> turns your BAML into a typed client for the
        language you already ship, and works with any model.
      </p>
      <p className="mt-3 text-sm text-white/50">
        Runs standalone on macOS, Linux and Windows. C# is coming soon.
      </p>
      <dl className="mt-12 border-t border-white/12">
        {ROWS.map((r) => (
          <div
            className="grid md:grid-cols-[240px_1fr] gap-4 py-6 md:py-8 border-b border-white/12"
            key={r.label}
          >
            <dt className="pt-2 text-sm text-white/55">{r.label}</dt>
            <dd className="flex flex-wrap gap-2">
              {r.items.map((item) => (
                <span
                  className="py-2 px-4 rounded-full border border-white/15 text-[15px] transition-colors hover:border-clover hover:text-clover"
                  key={item}
                >
                  {item}
                </span>
              ))}
            </dd>
          </div>
        ))}
      </dl>
    </section>
  );
}
