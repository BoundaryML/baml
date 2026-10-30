'use client';

import comboWhite from '@boundaryml/brand/logo/svg/combination-mark-white.svg';
import Image from 'next/image';
import { useState } from 'react';
import { cta } from '../analytics';
import BamlLetters from '../baml-letters';
import { track } from '../track';
import { Arrow } from './nav';

// Why add BAML to the traces you already send. A blank cell means the data isn't there by default, and
// cells that depend on your own instrumentation say so.
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
      <BamlLetters />
      <div className="relative max-w-[1440px] mx-auto px-5 md:px-12 py-20 md:py-36">
        <Image
          alt="BAML and Boundary"
          className="w-16 h-auto"
          src={comboWhite}
        />
        <h2 className="mt-7 max-w-300 font-pixel text-[clamp(38px,6vw,92px)] leading-[1.02] tracking-[-0.03em] text-clover">
          Use BAML for more powerful queries.
        </h2>
        <div className="flex flex-col md:flex-row md:items-end md:justify-between gap-7 mt-7">
          <p className="max-w-130 text-xl leading-[1.5] text-clover-100/80">
            We also built a small programming language that plugs in directly
            with Boundary Cloud called BAML. BAML programs emit even richer data
            -- more efficiently than OTEL.
          </p>
          <div className="flex gap-2.5 text-[15px] leading-4.5 font-medium">
            <a
              href="/what-is-baml"
              {...cta('explore_baml', 'baml')}
              className="group py-3.5 px-6 rounded-full bg-clover text-black hover:bg-clover-100 active:translate-y-px transition"
            >
              Explore BAML <Arrow />
            </a>
            <a
              href="https://github.com/boundaryml/baml"
              {...cta('github', 'baml')}
              className="py-3.5 px-6 rounded-full border border-clover/30 text-clover hover:border-clover/70 active:translate-y-px transition"
            >
              Star on GitHub
            </a>
          </div>
        </div>

        <div className="grid lg:grid-cols-[1.7fr_1fr] gap-4 mt-12 md:mt-16">
          <div className="min-w-0 rounded-2xl overflow-hidden bg-black/60 border border-clover/20">
            <div className="grid grid-cols-[1.25fr_1fr_1fr] gap-3 md:gap-4 px-4 md:px-6 py-4 border-b border-clover/20 text-sm text-clover-100/60">
              <span>What Boundary receives</span>
              <span>OTEL</span>
              <span className="text-clover">BAML</span>
            </div>
            <dl>
              {COMPARISON.map(([label, otel, baml]) => (
                <div
                  className="grid grid-cols-[1.25fr_1fr_1fr] gap-3 md:gap-4 px-4 md:px-6 py-3.5 border-b border-clover/10 last:border-0 text-[13px] leading-[19px] md:text-[15px] md:leading-[22px]"
                  key={label}
                >
                  <dt className="text-white">{label}</dt>
                  <dd className="text-white/50">
                    {otel || <span className="sr-only">Not recorded</span>}
                  </dd>
                  <dd
                    className={`text-clover-100 ${baml.startsWith('baml ') ? 'font-mono text-[12px] md:text-[13px]' : ''}`}
                  >
                    {baml}
                  </dd>
                </div>
              ))}
            </dl>
          </div>

          <div className="flex flex-col gap-4">
            {/* Drawn to make the point about scale: the recording cost barely registers beside a model call. */}
            <div className="flex-1 rounded-2xl p-6 md:p-7 bg-black/60 border border-clover/20">
              <div className="text-lg tracking-[-0.01em]">
                Recording cost per call
              </div>
              <div className="mt-6 text-sm text-clover-100/70">
                BAML, about 24 ns, or 60 ns with captured values
              </div>
              <div className="mt-2 w-[2px] h-3 bg-clover shadow-[0_0_8px_var(--color-clover-core)]" />
              <div className="mt-6 text-sm text-clover-100/70">
                A model call, hundreds of milliseconds
              </div>
              <div className="mt-2 h-3 rounded-[2px] bg-white/25" />
            </div>
            <div className="flex items-center justify-between rounded-2xl px-6 py-5 bg-black/60 border border-clover/20">
              <code className="font-mono text-sm text-clover">
                $ brew install baml
              </code>
              <button
                className="text-sm text-white/60 hover:text-white transition-colors cursor-pointer"
                onClick={copy}
                type="button"
              >
                {copied ? 'Copied' : 'Copy'}
              </button>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
