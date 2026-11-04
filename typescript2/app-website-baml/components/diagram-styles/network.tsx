'use client';

import Image from 'next/image';
import { Beam, Frame, Lamb, useBox, wire } from './kit';

// Your code on the left, BAML in the middle, models on the right, with beams running along the wires
// between them. Positions are shares of the frame, and the wires are drawn in its pixel space.
const LANGS = [
  { icon: '/langs/python.png', name: 'Python', y: 0.22 },
  { icon: '/langs/typescript.svg', name: 'TypeScript', y: 0.5 },
  { icon: '/langs/go.svg', name: 'Go', y: 0.78 },
];
const MODELS = [
  { mark: 'O', name: 'OpenAI', y: 0.16 },
  { mark: 'A', name: 'Anthropic', y: 0.39 },
  { mark: 'G', name: 'Gemini', y: 0.61 },
  { mark: 'B', name: 'Bedrock', y: 0.84 },
];
const LEFT = 0.13;
const CENTER = 0.5;
const RIGHT = 0.82;

export default function Network() {
  const [ref, { h, w }] = useBox<HTMLDivElement>();
  const hub = 48;
  const node = w < 640 ? 18 : 24;

  return (
    <Frame label="Python, TypeScript, and Go code calls a BAML function, which calls OpenAI, Anthropic, Gemini, or Bedrock. Beams run along the wires between them.">
      <div
        className="relative h-[300px] @min-[640px]:h-[380px] @min-[768px]:h-[440px]"
        ref={ref}
      >
        {w > 0 && (
          <svg
            aria-hidden="true"
            className="absolute inset-0"
            height={h}
            width={w}
          >
            {LANGS.map((l, i) => {
              const d = wire(LEFT * w + node, l.y * h, CENTER * w - hub, h / 2);
              return (
                <g key={l.name}>
                  <path
                    d={d}
                    fill="none"
                    stroke="rgb(from var(--ds-ink) r g b / 0.1)"
                  />
                  <Beam d={d} delay={i * 0.7} dur={2.8} />
                </g>
              );
            })}
            {MODELS.map((m, i) => {
              const d = wire(
                CENTER * w + hub,
                h / 2,
                RIGHT * w - node,
                m.y * h,
              );
              return (
                <g key={m.name}>
                  <path
                    d={d}
                    fill="none"
                    stroke="rgb(from var(--ds-ink) r g b / 0.1)"
                  />
                  <Beam d={d} delay={1.2 + i * 0.6} dur={2.8} />
                </g>
              );
            })}
          </svg>
        )}

        {LANGS.map((l) => (
          <div
            className="absolute -translate-y-1/2 flex items-center gap-3"
            key={l.name}
            style={{
              left: `calc(${LEFT * 100}% - ${node}px)`,
              top: `${l.y * 100}%`,
            }}
          >
            <span className="grid place-items-center size-9 @min-[640px]:size-12 rounded-xl border border-(--ds-line) bg-(--ds-card) shadow-[inset_0_1px_0_rgb(from_var(--ds-ink)_r_g_b/0.05)]">
              <Image
                alt=""
                className="size-4 @min-[640px]:size-5 object-contain"
                height={20}
                src={l.icon}
                width={20}
              />
            </span>
            <span className="hidden @min-[1024px]:block text-[13px] text-(--ds-ink)/60">
              {l.name}
            </span>
          </div>
        ))}

        <div className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 flex flex-col items-center">
          <span className="relative grid place-items-center size-20 @min-[640px]:size-24 rounded-2xl border border-(--ds-accent)/40 bg-(--ds-card) shadow-[0_0_0_6px_rgb(from_var(--ds-accent)_r_g_b/0.05),0_0_60px_-10px_rgb(from_var(--ds-accent)_r_g_b/0.5)]">
            <Lamb size={40} />
          </span>
          <span className="absolute top-full mt-3 font-mono text-[12px] @min-[640px]:text-[13px] text-(--ds-ink)/60 whitespace-nowrap">
            classify(text) -&gt; Verdict
          </span>
        </div>

        {MODELS.map((m) => (
          <div
            className="absolute -translate-y-1/2 flex items-center gap-3"
            key={m.name}
            style={{
              left: `calc(${RIGHT * 100}% - ${node}px)`,
              top: `${m.y * 100}%`,
            }}
          >
            <span className="grid place-items-center size-9 @min-[640px]:size-12 rounded-xl border border-(--ds-line) bg-(--ds-card) shadow-[inset_0_1px_0_rgb(from_var(--ds-ink)_r_g_b/0.05)] font-display font-medium text-[15px] @min-[640px]:text-[17px] text-(--ds-ink)/85">
              {m.mark}
            </span>
            <span className="hidden @min-[640px]:block text-[13px] text-(--ds-ink)/60">
              {m.name}
            </span>
          </div>
        ))}

        <p className="absolute left-4 top-4 @min-[640px]:left-6 @min-[640px]:top-5 text-[12px] text-(--ds-ink)/40">
          Your code
        </p>
        <p className="absolute right-4 top-4 @min-[640px]:right-6 @min-[640px]:top-5 text-[12px] text-(--ds-ink)/40">
          Any model
        </p>
      </div>
    </Frame>
  );
}
