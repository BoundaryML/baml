/** biome-ignore-all lint/suspicious/noArrayIndexKey: the lines are static and never reorder */
import Image from 'next/image';
import { section } from './analytics';
import Reveal from './reveal';
import { Arrow, Cmd, LANES, LEFT, LINKS, WRAP } from './ui';

// Illustrative timings, nested so they add up (main ⊃ ExtractInvoice ⊃ LLM call + parse).
const TRACE = [
  { ms: '412ms', name: 'main', root: true, start: 0, width: 64 },
  { ms: '401ms', name: 'ExtractInvoice', start: 2, width: 61 },
  { ms: '388ms', name: 'LLM call', start: 4, width: 56 },
  { ms: '4ms', name: 'parse → Invoice', start: 60, width: 2 },
];

const SESSION = `$ baml agent install
  agent guidance installed for this project's BAML version

$ baml describe ExtractInvoice
  function ExtractInvoice(file: pdf) -> Invoice
  extract.baml:7 · 2 tests

$ baml run main
  Invoice { vendor: "Acme", total: 1240.00 }`.split('\n');

export default function Toolchain() {
  return (
    <section
      {...section('toolchain')}
      aria-labelledby="toolchain-title"
      className="on-paint relative overflow-hidden"
    >
      <Image
        alt=""
        className="object-cover object-[50%_60%]"
        fill
        quality={90}
        sizes="100vw"
        src="/paint/baml-dusk-field.jpg"
      />
      {/* The same scrim as the closing band, so the field runs the full width. */}
      <div className="absolute inset-0 bg-[rgb(var(--scrim)/0.72)] lg:bg-transparent lg:bg-[linear-gradient(90deg,rgb(var(--scrim)/0.78)_0%,rgb(var(--scrim)/0.3)_55%,rgb(var(--scrim)/0)_100%)]" />

      <div
        className={`relative ${WRAP} py-16 md:py-24 lg:py-36 ${LANES} lg:items-start`}
      >
        <div className={`${LEFT} flex flex-col gap-10`}>
          <h2 className="h2 lg:max-w-[440px]" id="toolchain-title">
            Your agent drives the toolchain.
          </h2>
          <div className="flex flex-col gap-3 pt-[22px] border-t border-rule-accent">
            <p className="text-[15px] leading-6 text-body">
              <Cmd>baml agent install</Cmd> adds guidance matched to your
              project&apos;s BAML version. From there an agent can find code
              with <Cmd>baml describe</Cmd>, run any function with{' '}
              <Cmd>baml run</Cmd>, and test it before you ever look.
            </p>
            <a
              className="tap group self-start text-sm text-accent"
              href={LINKS.quickstart}
            >
              Read the quickstart <Arrow />
            </a>
          </div>
        </div>

        <Reveal className="flex-1 min-w-0">
          <figure className="rounded-xl bg-raised border border-white/10 shadow-[0_32px_64px_-16px_rgb(5_5_20/0.7)] overflow-hidden">
            <div className="flex items-center justify-between gap-4 pl-2.5 pr-3.5 py-2.5 border-b border-white/8 text-xs">
              <span className="flex gap-1 font-mono">
                <span className="px-2.5 py-1.5 rounded-md bg-white/7 text-ink">
                  zsh
                </span>
                <span className="px-2.5 py-1.5 text-muted">~/invoices</span>
              </span>
              <span className="text-[13px] text-muted">Copy</span>
            </div>
            {/* Line by line so each one can wrap under itself; one block with newlines could only
                take a hanging indent on its very first line. */}
            <pre className="code-pane px-5 sm:px-[26px] py-6 font-mono text-[12.5px] sm:text-sm leading-6 text-[#e8e8e4]">
              {SESSION.map((l, i) => (
                <div className="code-line" key={i}>
                  {l || ' '}
                </div>
              ))}
            </pre>
            <figcaption className="flex flex-col gap-2.5 px-5 sm:px-[26px] pt-5 pb-[26px] border-t border-white/8 bg-clover/4 font-mono text-xs">
              <span className="flex justify-between gap-4 font-sans text-[13px] text-muted">
                <span>Recorded automatically, no SDK</span>
                <span className="max-sm:hidden">baml run main</span>
              </span>
              {/* Three columns from sm. On a phone the bar track was squeezed to about 80px, where
                  a 64% span and a 61% span are the same picture and the 2% one is a tick — so the
                  waterfall drops to two rows and the bar gets the full width, which is the only
                  width at which it says anything. The name stops being truncated as well. */}
              {TRACE.map((t) => (
                <span
                  className="grid grid-cols-[1fr_auto] sm:grid-cols-[150px_1fr_52px] items-center gap-x-3.5 gap-y-1.5 text-body"
                  key={t.name}
                >
                  <span className={`truncate ${t.root ? '' : 'pl-3.5'}`}>
                    {t.name}
                  </span>
                  <span className="sm:order-3 text-right">{t.ms}</span>
                  <span
                    aria-hidden
                    className="col-span-2 sm:col-span-1 sm:order-2 relative h-2"
                  >
                    <span
                      className={`absolute inset-y-0 rounded ${t.root ? 'bg-clover' : 'bg-clover/45'}`}
                      style={{ left: `${t.start}%`, width: `${t.width}%` }}
                    />
                  </span>
                </span>
              ))}
            </figcaption>
          </figure>
        </Reveal>
      </div>
    </section>
  );
}
