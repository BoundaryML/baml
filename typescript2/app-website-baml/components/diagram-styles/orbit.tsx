import Image from 'next/image';
import type { CSSProperties } from 'react';
import { Frame, Lamb } from './kit';

// BAML at the center of what it works with: the languages that call it on the inner ring, the model
// providers it calls on the outer ring. The rings turn slowly in opposite directions, and every badge
// turns back against its ring so it stays upright.
const INNER = [
  { icon: '/langs/python.png', name: 'Python' },
  { icon: '/langs/typescript.svg', name: 'TypeScript' },
  { icon: '/langs/go.svg', name: 'Go' },
  { icon: '/langs/rust-orange.svg', name: 'Rust' },
];
const OUTER = [
  'OpenAI',
  'Anthropic',
  'Gemini',
  'Bedrock',
  'Azure',
  'Vertex',
  'Ollama',
  'OpenRouter',
];

function Ring({
  children,
  dur,
  reverse,
  size,
}: {
  children: React.ReactNode;
  dur: number;
  reverse?: boolean;
  size: number;
}) {
  return (
    <div
      className={`absolute left-1/2 top-1/2 rounded-full border border-(--ds-ink)/[0.08] ${reverse ? 'ds-orbit-back' : 'ds-orbit'}`}
      style={
        {
          '--dur': `${dur}s`,
          height: size,
          marginLeft: -size / 2,
          marginTop: -size / 2,
          width: size,
        } as CSSProperties
      }
    >
      {children}
    </div>
  );
}

function Seat({
  angle,
  children,
  dur,
  reverse,
}: {
  angle: number;
  children: React.ReactNode;
  dur: number;
  reverse?: boolean;
}) {
  return (
    <div
      className="absolute left-1/2 top-1/2 size-0"
      style={{
        transform: `rotate(${angle}deg) translateY(calc(var(--r) * -1)) rotate(${-angle}deg)`,
      }}
    >
      {/* Turns against the ring at the same speed, so the badge stays level. */}
      <div
        className={`absolute -translate-x-1/2 -translate-y-1/2 ${reverse ? 'ds-orbit' : 'ds-orbit-back'}`}
        style={{ '--dur': `${dur}s` } as CSSProperties}
      >
        {children}
      </div>
    </div>
  );
}

export default function Orbit() {
  return (
    <Frame label="BAML at the center. The languages that call it, Python, TypeScript, Go, and Rust, circle it on an inner ring, and the providers it calls, OpenAI, Anthropic, Gemini, Bedrock, Azure, Vertex, Ollama, and OpenRouter, circle on an outer ring.">
      <div className="relative h-[380px] @min-[640px]:h-[480px] @min-[768px]:h-[560px] [--outer:min(520px,92vw)] [--inner:min(300px,54vw)]">
        <div className="absolute inset-0 scale-[0.54] @min-[640px]:scale-[0.85] @min-[768px]:scale-100">
          <Ring dur={120} size={520}>
            <div className="contents [--r:260px]">
              {OUTER.map((name, i) => (
                <Seat
                  angle={(i * 360) / OUTER.length + 22}
                  dur={120}
                  key={name}
                >
                  <span className="inline-flex items-center h-8 px-3 rounded-full border border-(--ds-line) bg-(--ds-card) text-[13px] text-(--ds-ink)/75 whitespace-nowrap shadow-[inset_0_1px_0_rgb(from_var(--ds-ink)_r_g_b/0.05)]">
                    {name}
                  </span>
                </Seat>
              ))}
            </div>
          </Ring>
          <Ring dur={80} reverse size={300}>
            <div className="contents [--r:150px]">
              {INNER.map((l, i) => (
                <Seat
                  angle={(i * 360) / INNER.length + 45}
                  dur={80}
                  key={l.name}
                  reverse
                >
                  <span className="grid place-items-center size-12 rounded-xl border border-(--ds-line) bg-(--ds-card) shadow-[inset_0_1px_0_rgb(from_var(--ds-ink)_r_g_b/0.05)]">
                    <Image
                      alt={l.name}
                      className="size-5 object-contain"
                      height={20}
                      src={l.icon}
                      width={20}
                    />
                  </span>
                </Seat>
              ))}
            </div>
          </Ring>
          <div className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 grid place-items-center size-24 rounded-[22px] border border-(--ds-accent)/40 bg-(--ds-card) shadow-[0_0_0_8px_rgb(from_var(--ds-accent)_r_g_b/0.04),0_0_80px_-10px_rgb(from_var(--ds-accent)_r_g_b/0.55)]">
            <Lamb size={44} />
          </div>
        </div>
      </div>
    </Frame>
  );
}
