import { cta } from './analytics';
import BoardDemo from './board/board-demo';
import { LogTicker, ParallaxImage, PixelWord } from './motion';
import { BOOK_MEETING } from './page-parts';

// Faint wall of captured calls behind the headline.
const CALLS = [
  [
    '06:59:58.411',
    'user.ScoreLead',
    'baml_id_1_K3DEG1DQCF54ESF5DHRD',
    '383ms',
    '06:59:57.040',
    'user.SummarizeCall',
    'baml_id_1_CJW4KHXNGP1GEDQ95Y77',
    '1197ms',
  ],
  [
    '06:59:55.887',
    'user.ParseReceipt',
    'baml_id_1_SFX9Z6WEH4MZK94CEYZ0',
    '2048ms',
    '06:59:55.604',
    'user.ClassifyTicket',
    'baml_id_1_FV8EDX6W20B70MH9DQWJ',
    '1809ms',
  ],
  [
    '06:59:54.935',
    'user.ValidateVatId',
    'baml_id_1_9FM63VJ5V402RKFNKRRA',
    '1256ms',
    '06:59:54.397',
    'user.RouteEmail',
    'baml_id_1_K41YJD73333G83DPEQ6M',
    '395ms',
  ],
  [
    '06:59:54.074',
    'user.ProcessInvoice',
    'baml_id_1_KG1BEQ2KT018HH9788XF',
    '1583ms',
    '06:59:52.985',
    'user.ParseReceipt',
    'baml_id_1_8MBQ1KBXFT1M0RZRPS3R',
    '1636ms',
  ],
  [
    '06:59:51.908',
    'user.RouteEmail',
    'baml_id_1_BV8TP0601FRGR8PZQ8A8',
    '527ms',
    '06:59:50.707',
    'user.RouteEmail',
    'baml_id_1_H2P8N5ZF373FMMJBK7K8',
    '818ms',
  ],
  [
    '06:59:49.848',
    'user.ScoreLead',
    'baml_id_1_JBAGJ5PQBTQWSYT4JD07',
    '2234ms',
    '06:59:49.491',
    'user.ParseReceipt',
    '',
    '',
  ],
  [
    '06:59:48.379',
    'user.ClassifyTicket',
    'baml_id_1_E6YPV68STP6J4H36YES5',
    '1420ms',
    '06:59:47.239',
    'user.RouteEmail',
    '',
    '',
  ],
  [
    '06:59:46.633',
    'user.ParseReceipt',
    'baml_id_1_76B2ZWEHRGFTVCNVJ5T3',
    '1519ms',
    '06:59:46.326',
    'user.ClassifyTicket',
    '',
    '',
  ],
  [
    '06:59:44.963',
    'user.ProcessInvoice',
    'baml_id_1_TDNPXXQW6NV0BTCABP8S',
    '1950ms',
    '06:59:43.975',
    'user.ClassifyTicket',
    '',
    '',
  ],
  [
    '06:59:43.721',
    'user.ClassifyTicket',
    'baml_id_1_NMV6AT1ZYSCXQ0NAZ2F8',
    '1196ms',
    '06:59:42.915',
    'user.ScoreLead',
    '',
    '',
  ],
  [
    '06:59:42.548',
    'user.ParseReceipt',
    'baml_id_1_BRFBCJ1G26DBS9TA7EFE',
    '1212ms',
    '06:59:41.377',
    'user.ParseReceipt',
    '',
    '',
  ],
];
const pad = (s: string, n: number) => s.padEnd(n);
const callLine = (r: string[]) =>
  [
    pad(r[0], 14),
    pad(r[1], 22),
    pad(r[2], 33),
    pad(r[3] ? `${r[3].padStart(6)}  ok` : '', 20),
    pad(r[4], 14),
    pad(r[5], 22),
    r[6] ? `${pad(r[6], 33)}${r[7].padStart(6)}  ok` : '',
  ].join('');

export default function Hero() {
  return (
    <section className="w-full max-w-[1440px] mx-auto">
      <div className="relative flex flex-col lg:flex-row lg:justify-between lg:items-start gap-10 pt-16 md:pt-26 pb-12 md:pb-18 px-5 md:px-12 overflow-hidden">
        <LogTicker
          className="absolute left-5 md:left-12 top-1 h-[198px] overflow-hidden font-mono text-[12px] leading-[18px] text-ink/5 pointer-events-none select-none"
          initial={CALLS.map(callLine)}
        />
        <h1 className="relative max-w-215 font-display font-normal text-[clamp(44px,6vw,80px)] leading-[1.1] tracking-[-0.03em] text-ink text-balance">
          Agentic <PixelWord className="font-normal" word="Observability" />
        </h1>
        <div className="relative flex flex-col gap-7 lg:w-95 lg:pt-2 shrink-0">
          <p className="text-lg leading-7 tracking-[-0.005em] text-ink-2">
            Query OTEL traces to create alerts, dashboards, AI evals, or build
            your own tooling. Write your AI workflows in the BAML language to
            record richer, but cheaper telemetry data built for agents.
          </p>
          <div className="flex gap-2.5 text-[15px] leading-4.5 font-medium">
            <a
              href={BOOK_MEETING}
              {...cta('book_meeting', 'hero')}
              className="focus-ring py-3.25 px-5 rounded-md bg-primary text-white hover:bg-primary-hover active:translate-y-px transition"
            >
              Book a meeting
            </a>
            <a
              // biome-ignore lint/a11y/useValidAnchor: sign-up has no URL yet
              href="#"
              {...cta('start_free', 'hero')}
              className="py-3.25 px-5 rounded-md border border-line text-ink hover:border-ink-3 active:translate-y-px transition"
            >
              Start free
            </a>
          </div>
        </div>
      </div>

      <div className="px-5 md:px-12 pb-12">
        {/* Painting behind the product shot; the board sits inset 112px at 1344 wide (8.33%). */}
        <div className="relative overflow-hidden rounded-xl bg-pasture p-4 sm:p-[5%] md:p-[8.333%]">
          <ParallaxImage
            priority
            sizes="(min-width: 1440px) 1344px, 100vw"
            src="/boundary/painting.jpg"
          />
          <div className="relative rounded-[10px] overflow-hidden border border-black/8 bg-white dark:bg-[#121315] dark:border-white/10 shadow-[0_1px_2px_rgba(30,40,20,0.10),0_24px_60px_-12px_rgba(30,40,20,0.35)]">
            {/* phones: crop to the top-left panel only (board x 284, y 192, 701×482 of 1712×1024) so it stays legible */}
            <div className="max-md:aspect-[701/482] max-md:overflow-hidden">
              <div className="max-md:w-[244.22%] max-md:ml-[-40.51%] max-md:mt-[-27.39%]">
                <BoardDemo />
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
