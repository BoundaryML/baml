'use client';

import { useState } from 'react';
import { SIGNUP } from '@/components/home/links';

// The top of /adopt: getting in touch, the primary action, then one install command and a button
// that copies a setup prompt for an agent. No login
// step: Boundary Cloud goes through early access. Every command is the CLI's own (`baml --help`);
// `baml agent install` needs no project.

const INSTALL = 'curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s';

const PROMPT = `Set up BAML in this repo.

1. If the baml CLI isn't installed, install it:
   curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s
   (or \`brew install baml\`; on Windows: irm https://pkg.boundaryml.com/install.ps1 | iex)
2. Run \`baml agent install\`, then use the BAML skill and \`baml describe\` to learn the language.
3. Be ambitious. BAML is a full programming language, not a prompt format: move whole workflows into it, the model calls and the logic around them, not just single prompts. Start with the part of this codebase that would gain the most.
4. Call it from our code: \`baml generate add <language>\`, then \`baml generate\`.
5. Keep it working: run \`baml check\` and \`baml fmt\` after every change, write tests and run them with \`baml test\`, and try functions with \`baml run\`.
6. When something looks wrong, inspect the recorded calls with \`baml query\` (SQL) instead of guessing.`;

/** A button that copies `text`, and says so for a moment. */
function useCopied(): [boolean, (text: string) => void] {
  const [done, setDone] = useState(false);
  return [
    done,
    (text) => {
      navigator.clipboard?.writeText(text).then(
        () => {
          setDone(true);
          setTimeout(() => setDone(false), 1500);
        },
        () => {},
      );
    },
  ];
}

/** The page's one primary action, getting in touch, with why; then the install command and a button
 *  that copies a setup prompt for an agent. The prompt goes to the clipboard, not onto the page. */
export function Start() {
  const [installed, copyInstall] = useCopied();
  const [prompted, copyPrompt] = useCopied();
  return (
    <div className="mt-8 md:mt-10">
      <div className="flex flex-wrap items-center gap-x-5 gap-y-3">
        <a
          className="focus-ring inline-flex h-[46px] items-center rounded-lg bg-accent px-5 text-[15px] font-medium text-on-accent transition hover:bg-accent-hover active:translate-y-px"
          href={SIGNUP}
          rel="noopener"
          target="_blank"
        >
          Get in touch
        </a>
        <p className="max-w-[34rem] text-[15px] leading-snug text-ink-2">
          Not sure BAML fits your use case? Talk to us. Our docs are still
          catching up while we focus on the engineering, so we’d rather help you
          directly.
        </p>
      </div>
      <div className="mt-8 flex flex-wrap items-center gap-3">
        <div className="flex min-w-0 max-w-full items-center gap-3 rounded-xl border border-rule bg-raised py-1.5 pr-1.5 pl-4">
          <code className="min-w-0 overflow-x-auto whitespace-nowrap font-mono text-[13px] text-ink">
            <span className="text-muted">$ </span>
            {INSTALL}
          </code>
          <button
            className="focus-ring shrink-0 cursor-pointer rounded-lg px-2.5 py-1.5 text-[13px] text-muted transition-colors hover:bg-white/10 hover:text-head"
            onClick={() => copyInstall(INSTALL)}
            type="button"
          >
            {installed ? 'Copied' : 'Copy'}
          </button>
        </div>
        <button
          className="focus-ring cursor-pointer rounded-xl border border-rule px-4 py-2.5 text-[14px] text-ink transition-colors hover:border-white/30 hover:text-head"
          onClick={() => copyPrompt(PROMPT)}
          type="button"
        >
          {prompted
            ? 'Copied. Paste it into your agent'
            : 'Copy the setup prompt for your agent'}
        </button>
      </div>
      <p className="mt-2.5 text-[13px] text-muted">
        On Windows, or with Homebrew?{' '}
        <a
          className="underline underline-offset-2 hover:text-head"
          href="/quickstart"
        >
          Other ways to install
        </a>
      </p>
    </div>
  );
}
