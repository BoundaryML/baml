'use client';

import { useState } from 'react';
import { Rise } from './motion';
import { track } from './track';

const QUESTIONS: [string, string][] = [
  [
    'How is this different from Datadog or Langfuse?',
    'Each of those covers one stage, and you join them by hand. Boundary gives you the traces and logs of any OpenTelemetry platform, adds AI-native features, and keeps writing, testing, releasing, observing and comparing in one place, joined by your function names. That shared data is what lets an agent run the loop.',
  ],
  [
    'Do I have to write my code in BAML?',
    'No. BAML is free and generates typed SDKs for Python and TypeScript, so you can adopt it one function at a time and instrument any part of your code. The parts written in BAML are traced automatically, with no SDK.',
  ],
  [
    'Where does my data live?',
    'In one place, hosted by Boundary or in your own AWS. Your cloud, your data. People query it from Studio, the CLI or curl, and agents with a read key.',
  ],
  [
    'What gets captured, and what stays private?',
    'Every call is profiled. Inputs, outputs and errors are captured when you opt in, and private values never leave your data plane.',
  ],
  [
    'Is BAML ready for production?',
    "BAML is open source and pre-1.0. It's usable today, but some APIs and tooling may still change, and the agent skill, the CLI and baml describe always match the version you have installed.",
  ],
  [
    "What ships today, and what's coming?",
    'Writing in BAML, tracing, SQL queries and Studio dashboards ship today. Feature flags are next: ramp a change to a share of users, or switch what the code does for one customer in an emergency. Datasets are coming too.',
  ],
];

export default function Faq() {
  const [open, setOpen] = useState(0);
  return (
    <section
      className="w-full max-w-[1440px] mx-auto flex flex-col lg:flex-row gap-12 lg:gap-16 px-5 md:px-12 pt-16 md:pt-30 pb-20 md:pb-34 border-t border-hair"
      id="docs"
    >
      <div className="lg:w-105 shrink-0 flex flex-col gap-5">
        <Rise className="h2-display" text="Asked a lot." />
        <a
          className="self-start text-[15px] leading-[22px] text-ink-2 ulink hover:text-accent"
          // biome-ignore lint/a11y/useValidAnchor: the team's contact address isn't set yet
          href="#"
        >
          Can&apos;t find your question? Email the team
        </a>
      </div>
      <ul className="flex-1 border-t border-ink">
        {QUESTIONS.map(([q, a], i) => {
          const on = open === i;
          return (
            <li className="border-b border-hair" key={q}>
              <button
                aria-expanded={on}
                className="w-full flex justify-between gap-6 py-6 text-left cursor-pointer"
                onClick={() => {
                  if (!on) track('faq_opened', { index: i, question: q });
                  setOpen(on ? -1 : i);
                }}
                type="button"
              >
                <span className="font-display font-medium text-[19px] leading-6 tracking-[-0.03em] text-ink">
                  {q}
                </span>
                <span
                  aria-hidden
                  className="text-xl leading-6 text-ink w-4 text-center"
                >
                  {on ? '−' : '+'}
                </span>
              </button>
              {on && (
                <p className="max-w-160 pb-6 -mt-2 text-[15px] leading-6 text-ink-2">
                  {a}
                </p>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
