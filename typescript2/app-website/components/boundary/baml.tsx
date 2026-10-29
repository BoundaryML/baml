'use client';

import comboWhite from '@boundaryml/brand/logo/svg/combination-mark-white.svg';
import Image from 'next/image';
import { useState } from 'react';
import { cta } from './analytics';
import BamlLetters from './baml-letters';
import { Rise } from './motion';
import { Arrow } from './nav';
import { track } from './track';

// The same function the loop follows, so one name runs through the whole page.
// Same syntax as the BAML site (TypeScript-like, from boundaryml.com) and the same function the loop follows.
const CODE = `class Invoice {
  vendor: string,
  total: float,
  due_date: string?,
}

function ExtractInvoice(file: pdf) -> Invoice {
  client: "openai/gpt-4o-mini"
  prompt: \`
    Pull the fields out of \${file}.
    \${ctx.output_format()}
  \`
}`;

export default function Baml() {
  const [copied, setCopied] = useState(false);
  const copy = () => {
    track('install_command_copied', {
      command: 'brew install baml',
      location: 'baml',
    });
    navigator.clipboard?.writeText('brew install baml').then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };

  return (
    <section className="relative overflow-hidden bg-clover-900">
      {/* BAML is a dusk layout: the deep end of the Clover scale. */}
      {/* Scattered "BOUNDARY" letters; they glitch when hovered. */}
      <BamlLetters />
      <div className="relative max-w-[1440px] mx-auto flex flex-col lg:flex-row lg:items-center gap-14 lg:gap-24 px-5 md:px-12 py-18 md:py-34">
        <div className="lg:w-140 shrink-0 flex flex-col gap-7">
          {/* the combination mark (lamb over the gate), white: BAML and Boundary together */}
          <Image
            alt="BAML and Boundary"
            className="w-16 h-auto"
            src={comboWhite}
          />
          <Rise
            className="font-pixel text-[clamp(34px,3.4vw,46px)] leading-[1.1] tracking-[-0.02em] text-clover"
            text="Built on BAML, so your code and what you can see change together."
          />
          <p className="text-[17px] leading-[27px] text-clover-100/75">
            BAML is free and open source. It runs everywhere and plugs into
            everything: write functions with typed outputs, keep tests next to
            the code, and get tracing built in with no SDK, so every call
            reports straight to Boundary.
          </p>
          <div className="flex gap-2.5 pt-2 text-[15px] leading-4.5 font-medium">
            <a
              href="/what-is-baml"
              {...cta('explore_baml', 'baml')}
              className="group py-3.25 px-5 rounded-md bg-clover text-[#010101] hover:bg-clover-100 active:translate-y-px transition"
            >
              Explore BAML <Arrow />
            </a>
            <a
              href="https://github.com/boundaryml/baml"
              {...cta('github', 'baml')}
              className="py-3 px-5 rounded-md border border-clover/30 text-clover hover:border-clover/70 active:translate-y-px transition"
            >
              Star on GitHub
            </a>
          </div>
        </div>
        {/* the code window sits on the dusk painting (BAML = dusk, Boundary = daylight) */}
        <div className="relative flex-1 min-w-0 rounded-[14px] overflow-hidden bg-[#1E2A44] p-3 sm:p-5 md:p-10">
          <Image
            alt=""
            className="object-cover object-[50%_70%]"
            fill
            quality={90}
            sizes="(min-width: 1024px) 760px, 100vw"
            src="/boundary/painting-baml-dusk.jpg"
          />
          <div className="relative flex flex-col rounded-xl overflow-hidden bg-[#0F0F0F] border border-white/10 shadow-[0_1px_2px_rgba(5,10,25,0.3),0_24px_48px_-12px_rgba(5,10,25,0.6)]">
            <div className="flex gap-5 px-5 py-3.5 border-b border-[#2A2E28] font-mono text-xs">
              <span className="text-white">extract.baml</span>
              <span className="text-[#6E756A]">main.py</span>
            </div>
            <pre className="px-5 py-6 overflow-x-auto font-mono text-[12px] leading-5 md:text-sm md:leading-[23px] text-[#D8DDD3]">
              {CODE}
            </pre>
            <div className="flex justify-between px-5 py-3.5 border-t border-[#2A2E28]">
              <code className="font-mono text-[13px] text-clover">
                $ brew install baml
              </code>
              <button
                className="text-[13px] text-[#6E756A] hover:text-white transition-colors"
                onClick={copy}
                type="button"
              >
                {copied ? 'copied' : 'copy'}
              </button>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
