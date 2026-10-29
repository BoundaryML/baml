'use client';

import type { ReactNode } from 'react';
import { section } from './analytics';
import { track } from './track';
import { Cmd, LANES, LEFT, PAD_Y, WRAP } from './ui';

/* Objection handling sits between the Boundary band and the status block. Every answer is a fact
   already stated on boundaryml.com, /pricing or /quickstart — nothing here is new policy.
   The pre-1.0 question is deliberately about what changes do to an installed project, so it adds to
   the Status section below rather than repeating it.
   Native <details>, so the answers are in the HTML, open without JS and announce themselves. */
const QUESTIONS: { q: string; a: ReactNode }[] = [
  {
    a: (
      <>
        No. <Cmd>baml generate</Cmd> writes a typed client for Python,
        TypeScript, Go, Rust, Java, Swift or C++, and you call BAML functions
        from the code you already have. Adoption is one function at a time.
      </>
    ),
    q: 'Do I have to rewrite my app in BAML?',
  },
  {
    a: (
      <>
        Nothing. The language, the toolchain, SDK generation and the local
        playground are Apache 2.0 and run on your machine. Boundary — the hosted
        place BAML&apos;s calls can land — is a separate product, and BAML never
        requires it.
      </>
    ),
    q: 'What does BAML cost?',
  },
  {
    a: (
      <>
        OpenAI, Anthropic, Gemini, Vertex, Bedrock, Azure, Ollama, OpenRouter,
        Vercel, Claude Code, MCP tools, and anything compatible with them. The
        model is one line inside the function —{' '}
        <Cmd>client: &quot;openai/gpt-4o&quot;</Cmd> — so swapping it is a
        one-line diff and the tests beside the function tell you what it
        changed.
      </>
    ),
    q: 'Which models does it work with?',
  },
  {
    a: (
      <>
        To the provider you name in the function, and nowhere else by default.
        BAML runs on your machine and instruments every call without an SDK, but
        those traces only reach Boundary if you send them — hosted for you, or
        into your own AWS account.
      </>
    ),
    q: 'Where does my data go?',
  },
  {
    a: (
      <>
        <Cmd>baml ide install</Cmd> sets up VS Code and Cursor. Zed, JetBrains
        and Neovim connect over the language server, and{' '}
        <Cmd>baml playground</Cmd> opens one in the browser if you&apos;d rather
        not install anything.
      </>
    ),
    q: 'Does my editor understand it?',
  },
  {
    a: (
      <>
        BAML is pre-1.0, so some APIs and tooling will still move. The parts an
        agent reads are versioned with the binary: <Cmd>baml describe</Cmd> and
        the guidance that <Cmd>baml agent install</Cmd> writes always match the
        version you have, and the CLI prompts you to reinstall the guidance
        after an update, so an agent never works from a stale description of
        your project.
      </>
    ),
    q: 'What happens when BAML changes under me?',
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
        <h2 className="h2 lg:max-w-[420px]" id="faq-title">
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
