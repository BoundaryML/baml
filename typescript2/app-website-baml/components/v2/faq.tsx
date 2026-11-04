'use client';

import type { ReactNode } from 'react';
import { section } from '../analytics';
import { track } from '../track';
import { Cmd, LANES, LEFT, PAD_Y, WRAP } from '../ui';

/* Native <details>, so the answers are in the HTML, open without JS and announce themselves. */
const QUESTIONS: { q: string; a: ReactNode }[] = [
  {
    a: (
      <>
        Yes. BAML is in beta, which means the language and standard library may
        still change, but it&apos;s stable for production use. It doesn&apos;t
        have a package manager yet, and we plan to add one soon.
      </>
    ),
    q: 'Is BAML production ready?',
  },
  {
    a: (
      <>
        No. <Cmd>baml generate</Cmd> writes a typed client for Python,
        TypeScript, Go, Rust, Java, Swift, or C++, and you call BAML functions
        from your existing code. You can adopt BAML one function at a time.
      </>
    ),
    q: 'Do I have to rewrite my app in BAML?',
  },
  {
    a: (
      <>
        BAML is free. The language, the toolchain, SDK generation, and the local
        playground are licensed under Apache 2.0 and run on your machine.
        Boundary, the hosted service that stores recordings from production, is
        a separate product that BAML never requires.
      </>
    ),
    q: 'What does BAML cost?',
  },
  {
    a: (
      <>
        BAML works with OpenAI, Anthropic, Gemini, Vertex, Bedrock, Azure,
        Ollama, OpenRouter, Vercel, Claude Code, MCP tools, and any compatible
        API. The model is one line in the function, such as{' '}
        <Cmd>client: &quot;openai/gpt-4o&quot;</Cmd>, so switching models is a
        one-line change. The tests next to the function show what the change
        affects.
      </>
    ),
    q: 'Which models does it work with?',
  },
  {
    a: (
      <>
        Prompts go to the model provider named in the function. Recordings stay
        in files in your project, and they reach Boundary only if you add a key.
        Set <Cmd>BAML_TELEMETRY=off</Cmd> to turn recording off.
      </>
    ),
    q: 'Where does my data go?',
  },
  {
    a: (
      <>
        We&apos;re working on OpenTelemetry export, so you&apos;ll be able to
        send BAML telemetry to the tools you already use.
      </>
    ),
    q: 'Can I export telemetry elsewhere?',
  },
];

export default function Faq() {
  return (
    <section
      {...section('faq')}
      aria-labelledby="faq-title"
      className={`${WRAP} ${PAD_Y} ${LANES}`}
    >
      <div className={`${LEFT} flex flex-col gap-4`}>
        <h2 className="v2-h2 lg:max-w-[520px]" id="faq-title">
          Before you install.
        </h2>
      </div>

      <div className="flex-1 min-w-0">
        {QUESTIONS.map((item, i) => (
          <details
            className={`group border-t ${i === 0 ? 'border-rule-accent' : 'border-rule'} last:border-b last:border-b-rule`}
            key={item.q}
            name="faq"
            onToggle={(e) => {
              if (e.currentTarget.open)
                track('faq_opened', { index: i, question: item.q });
            }}
          >
            <summary className="flex justify-between items-start gap-6 py-[22px] cursor-pointer list-none [&::-webkit-details-marker]:hidden">
              <span className="font-display font-medium text-[clamp(18px,1.6vw,22px)] leading-[1.3] tracking-[-0.03em] text-pretty group-hover:text-accent group-open:text-accent transition-colors">
                {item.q}
              </span>
              <span
                aria-hidden
                className="shrink-0 mt-1.5 text-muted transition-transform duration-200 group-open:rotate-45"
              >
                <svg
                  aria-hidden="true"
                  fill="none"
                  height="16"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  viewBox="0 0 16 16"
                  width="16"
                >
                  <path d="M8 2.5v11M2.5 8h11" />
                </svg>
              </span>
            </summary>
            <p className="max-w-[620px] pb-7 -mt-1 text-[15px] leading-[25px] text-body text-pretty">
              {item.a}
            </p>
          </details>
        ))}
      </div>
    </section>
  );
}
