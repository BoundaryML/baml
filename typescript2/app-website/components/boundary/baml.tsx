'use client';

import comboWhite from '@boundaryml/brand/logo/svg/combination-mark-white.svg';
import Image from 'next/image';
import { useState } from 'react';
import { cta } from './analytics';
import BamlLetters from './baml-letters';
import { Rise } from './motion';
import { Arrow } from './nav';
import { track } from './track';

// Why add BAML to the traces you already send. Fair to OpenTelemetry: a blank cell means the data
// isn't there by default, and cells that depend on your own instrumentation say so.
const COMPARISON: [label: string, otel: string, baml: string][] = [
  ['Spans, timing, and errors', '✓', '✓'],
  [
    'Model requests and token usage',
    'When your instrumentation records them',
    'On every call',
  ],
  [
    'Arguments and results',
    'As the attributes you add',
    'Stored with their BAML types',
  ],
  ['The step that failed', '', 'Request, provider, or parse'],
  ['Exact counts for each call path', '', '✓'],
  ['The code version of each run', '', '✓'],
  [
    'Recording cost per call',
    'Depends on your SDK',
    'About 24 ns, or 60 ns with captured values',
  ],
  ['The same queries on your laptop', '', 'baml query'],
  ['Tests built from production calls', '', 'baml test'],
];

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
            text="Use BAML for more powerful queries."
          />
          <p className="text-[17px] leading-[27px] text-clover-100/75">
            BAML is the language for agents. BAML programs emit even richer
            data, more efficiently than OTEL.
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
            <div className="grid grid-cols-[1.25fr_1fr_1fr] gap-3 md:gap-4 px-4 md:px-5 py-3.5 border-b border-[#2A2E28] font-mono text-xs text-[#6E756A]">
              <span>What Boundary receives</span>
              <span>OTEL</span>
              <span className="text-clover">BAML</span>
            </div>
            <dl>
              {COMPARISON.map(([label, otel, baml]) => (
                <div
                  className="grid grid-cols-[1.25fr_1fr_1fr] gap-3 md:gap-4 px-4 md:px-5 py-3 border-b border-[#2A2E28] last:border-0 text-[12px] leading-[18px] md:text-[13px] md:leading-5"
                  key={label}
                >
                  <dt className="text-white">{label}</dt>
                  <dd className="text-[#8C9388]">
                    {otel || <span className="sr-only">Not recorded</span>}
                  </dd>
                  <dd
                    className={`text-clover-100 ${baml.startsWith('baml ') ? 'font-mono' : ''}`}
                  >
                    {baml}
                  </dd>
                </div>
              ))}
            </dl>
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
