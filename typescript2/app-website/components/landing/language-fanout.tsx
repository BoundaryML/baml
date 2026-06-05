'use client';

import { motion, useInView } from 'motion/react';
import Image from 'next/image';
import { useRef } from 'react';
import { SiGo, SiRuby, SiRust, SiTypescript, SiOpenjdk } from 'react-icons/si';
import type { IconType } from 'react-icons';

const BG = '#ffffff';
const INK = '#1A1612';
const MUTED = '#5C5852';
const BORDER = '#D9D3C4';
const ACCENT = '#6D28D9';
const ACCENT_SOFT = '#A78BFA';
const EYEBROW = '#8A8580';
const CARD_BG = '#FBF8F1';
const MONO =
  '"IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';

type Lang = {
  name: string;
  Icon?: IconType;
  color?: string;
  // When set, the tile renders this image (used for multi-color logos).
  image?: string;
  // When set, the tile renders this string in place of an icon.
  glyph?: string;
};

const LANGS: Lang[] = [
  { name: 'Python', image: '/python-icon.png' },
  { name: 'TypeScript', Icon: SiTypescript, color: '#3178C6' },
  { name: 'Ruby', Icon: SiRuby, color: '#CC342D' },
  { name: 'Go', Icon: SiGo, color: '#00ADD8' },
  { name: 'Rust', Icon: SiRust, color: '#000000' },
  { name: 'Java', Icon: SiOpenjdk, color: '#ED8B00' },
  { name: 'and more', glyph: '+' },
];

// Geometry — keep SVG and tile layout in lockstep.
const SVG_WIDTH = 880;
const SVG_HEIGHT = 240;
const TILE_COUNT = LANGS.length;

export function LanguageFanout() {
  const ref = useRef<HTMLDivElement>(null);
  const inView = useInView(ref, { amount: 0.3, once: true });
  // End-point x positions (each tile center, in viewBox coords).
  const slotWidth = SVG_WIDTH / TILE_COUNT;
  const tileXs = Array.from(
    { length: TILE_COUNT },
    (_, i) => slotWidth * (i + 0.5),
  );
  const startX = SVG_WIDTH / 2;
  const startY = 0;
  const endY = SVG_HEIGHT - 4;

  // Build path strings once.
  const paths = tileXs.map((tx) => {
    const cy1 = SVG_HEIGHT * 0.55;
    return `M ${startX} ${startY} C ${startX} ${cy1}, ${tx} ${cy1}, ${tx} ${endY}`;
  });

  return (
    <section
      ref={ref}
      aria-label="BAML compiles to every language"
      style={{
        background: BG,
        color: INK,
        padding: 'clamp(64px, 13vw, 128px) 0 clamp(80px, 16vw, 160px)',
        width: '100%',
        borderTop: `1px solid ${BORDER}`,
      }}
    >
      <div
        style={{
          maxWidth: 1360,
          margin: '0 auto',
          padding: '0 clamp(16px, 5vw, 32px)',
        }}
      >
        {/* Eyebrow + heading */}
        <div
          style={{
            textAlign: 'center',
            marginBottom: 'clamp(40px, 9vw, 80px)',
          }}
        >
          <p
            style={{
              fontSize: 13,
              fontWeight: 500,
              letterSpacing: '0.12em',
              textTransform: 'uppercase',
              color: EYEBROW,
              margin: 0,
            }}
          >
            One file. Every client.
          </p>
          <h2
            style={{
              fontSize: 'clamp(2rem, 4vw, 3.25rem)',
              fontWeight: 600,
              lineHeight: 1.08,
              letterSpacing: '-0.025em',
              margin: '14px 0 0',
              color: INK,
            }}
          >
            Write it once. Call it from{' '}
            <span
              style={{
                fontFamily: 'var(--font-serif)',
                fontStyle: 'italic',
                fontWeight: 500,
                color: ACCENT,
              }}
            >
              anywhere
            </span>
            .
          </h2>
          <p
            style={{
              fontSize: 17,
              lineHeight: 1.6,
              color: MUTED,
              margin: '20px auto 0',
              maxWidth: 580,
            }}
          >
            The BAML compiler turns a single{' '}
            <code style={{ fontFamily: MONO, fontSize: 14 }}>.baml</code> file
            into typed SDKs for every language your stack speaks.
          </p>
        </div>

        {/* Diagram */}
        <div
          style={{
            position: 'relative',
            margin: '0 auto',
            maxWidth: SVG_WIDTH,
            width: '100%',
          }}
        >
          {/* BAML source node — the lamb */}
          <div
            style={{
              display: 'flex',
              justifyContent: 'center',
              marginBottom: -8,
            }}
          >
            <motion.div
              initial={{ opacity: 0, y: -16, scale: 0.92 }}
              animate={inView ? { opacity: 1, y: 0, scale: 1 } : {}}
              transition={{ duration: 0.6, ease: [0.22, 0.61, 0.36, 1] }}
              style={{
                position: 'relative',
                display: 'inline-flex',
                flexDirection: 'column',
                alignItems: 'center',
                gap: 10,
                cursor: 'pointer',
              }}
            >
              {/* Soft halo behind the lamb */}
              <motion.div
                aria-hidden
                animate={
                  inView
                    ? {
                        scale: [1, 1.1, 1],
                        opacity: [0.45, 0.7, 0.45],
                      }
                    : {}
                }
                transition={{
                  duration: 3.2,
                  repeat: Infinity,
                  ease: 'easeInOut',
                }}
                style={{
                  position: 'absolute',
                  inset: -16,
                  borderRadius: '50%',
                  background:
                    'radial-gradient(circle, rgba(167,139,250,0.35) 0%, rgba(167,139,250,0) 70%)',
                  pointerEvents: 'none',
                  zIndex: 0,
                }}
              />
              <div
                style={{
                  position: 'relative',
                  width: 132,
                  height: 132,
                  borderRadius: 20,
                  background: CARD_BG,
                  border: `1px solid ${BORDER}`,
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  boxShadow:
                    '0 18px 40px -22px rgba(0,0,0,0.28), 0 1px 0 rgba(0,0,0,0.02)',
                  zIndex: 1,
                  overflow: 'hidden',
                }}
              >
                <Image
                  alt="BAML"
                  src="/bamllogopurple.svg"
                  width={108}
                  height={108}
                  priority
                  style={{
                    width: 108,
                    height: 108,
                    objectFit: 'contain',
                  }}
                />
              </div>
            </motion.div>
          </div>

          {/* Connector SVG */}
          <svg
            aria-hidden
            viewBox={`0 0 ${SVG_WIDTH} ${SVG_HEIGHT}`}
            preserveAspectRatio="none"
            style={{
              display: 'block',
              width: '100%',
              height: SVG_HEIGHT,
              marginTop: 8,
              overflow: 'visible',
            }}
          >
            <defs>
              <radialGradient id="lf-particle-glow">
                <stop offset="0%" stopColor={ACCENT} stopOpacity="0.9" />
                <stop offset="100%" stopColor={ACCENT} stopOpacity="0" />
              </radialGradient>
            </defs>

            {paths.map((d, i) => {
              const pulseDuration = 1.8 + (i % 3) * 0.2;
              const pulseDelay = 0.9 + i * 0.16;
              return (
                <g key={`flow-${i}`}>
                  {/* Static gray wire (drawn on entrance, then stays) */}
                  <motion.path
                    d={d}
                    fill="none"
                    stroke={BORDER}
                    strokeWidth={1.75}
                    strokeLinecap="round"
                    initial={{ pathLength: 0, opacity: 0 }}
                    animate={inView ? { pathLength: 1, opacity: 1 } : {}}
                    transition={{
                      duration: 0.7,
                      delay: 0.25 + i * 0.06,
                      ease: 'easeInOut',
                    }}
                  />
                  {/* Purple energy pulse — wide soft glow */}
                  <motion.path
                    d={d}
                    fill="none"
                    stroke={ACCENT_SOFT}
                    strokeWidth={6}
                    strokeLinecap="round"
                    strokeOpacity={0.45}
                    pathLength={1}
                    strokeDasharray="0.16 2"
                    initial={{ strokeDashoffset: 0.16, opacity: 0 }}
                    animate={
                      inView
                        ? {
                            strokeDashoffset: [0.16, -1.0],
                            opacity: [0, 1, 1, 0],
                          }
                        : {}
                    }
                    transition={{
                      strokeDashoffset: {
                        delay: pulseDelay,
                        duration: pulseDuration,
                        repeat: Infinity,
                        repeatDelay: 0.6,
                        ease: 'easeInOut',
                      },
                      opacity: {
                        delay: pulseDelay,
                        duration: pulseDuration,
                        times: [0, 0.15, 0.85, 1],
                        repeat: Infinity,
                        repeatDelay: 0.6,
                        ease: 'easeInOut',
                      },
                    }}
                  />
                  {/* Purple energy pulse — bright core */}
                  <motion.path
                    d={d}
                    fill="none"
                    stroke={ACCENT}
                    strokeWidth={2.25}
                    strokeLinecap="round"
                    pathLength={1}
                    strokeDasharray="0.10 2"
                    initial={{ strokeDashoffset: 0.1, opacity: 0 }}
                    animate={
                      inView
                        ? {
                            strokeDashoffset: [0.1, -1.0],
                            opacity: [0, 1, 1, 0],
                          }
                        : {}
                    }
                    transition={{
                      strokeDashoffset: {
                        delay: pulseDelay,
                        duration: pulseDuration,
                        repeat: Infinity,
                        repeatDelay: 0.6,
                        ease: 'easeInOut',
                      },
                      opacity: {
                        delay: pulseDelay,
                        duration: pulseDuration,
                        times: [0, 0.12, 0.88, 1],
                        repeat: Infinity,
                        repeatDelay: 0.6,
                        ease: 'easeInOut',
                      },
                    }}
                  />
                </g>
              );
            })}

            {/* Pulsing landing dots */}
            {tileXs.map((tx, i) => (
              <motion.g
                key={`dot-${i}`}
                initial={{ opacity: 0 }}
                animate={inView ? { opacity: 1 } : {}}
                transition={{ delay: 0.6 + i * 0.06, duration: 0.3 }}
              >
                <motion.circle
                  cx={tx}
                  cy={endY}
                  r={6}
                  fill={ACCENT}
                  fillOpacity={0.18}
                  animate={
                    inView
                      ? {
                          r: [6, 14, 6],
                          fillOpacity: [0.25, 0, 0.25],
                        }
                      : {}
                  }
                  transition={{
                    delay: 1.0 + i * 0.18,
                    duration: 1.6,
                    repeat: Infinity,
                    ease: 'easeOut',
                  }}
                />
                <circle cx={tx} cy={endY} r={3} fill={ACCENT} />
              </motion.g>
            ))}
          </svg>

          {/* Language tiles */}
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: `repeat(${TILE_COUNT}, minmax(0, 1fr))`,
              gap: 0,
              marginTop: 12,
            }}
          >
            {LANGS.map((lang, i) => (
              <motion.div
                key={lang.name}
                initial={{ opacity: 0, y: 12 }}
                animate={inView ? { opacity: 1, y: 0 } : {}}
                transition={{
                  duration: 0.45,
                  delay: 0.7 + i * 0.06,
                  ease: 'easeOut',
                }}
                style={{
                  display: 'flex',
                  flexDirection: 'column',
                  alignItems: 'center',
                  gap: 12,
                  cursor: 'pointer',
                }}
              >
                <div
                  className="lang-tile"
                  style={{
                    width: 'clamp(42px, 12vw, 84px)',
                    height: 'clamp(42px, 12vw, 84px)',
                    borderRadius: 'clamp(10px, 2.5vw, 16px)',
                    border: `1px solid ${BORDER}`,
                    background: BG,
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    boxShadow:
                      '0 1px 0 rgba(0,0,0,0.02), 0 8px 22px -16px rgba(0,0,0,0.18)',
                    transition:
                      'box-shadow 220ms ease, transform 220ms ease, border-color 220ms ease',
                  }}
                >
                  {lang.image ? (
                    <Image
                      alt={lang.name}
                      height={40}
                      src={lang.image}
                      style={{
                        height: '48%',
                        maxHeight: 40,
                        objectFit: 'contain',
                        width: '48%',
                        maxWidth: 40,
                      }}
                      width={40}
                    />
                  ) : lang.Icon ? (
                    <span
                      style={{
                        alignItems: 'center',
                        color: lang.color,
                        display: 'flex',
                        fontSize: 'clamp(22px, 6vw, 40px)',
                        justifyContent: 'center',
                      }}
                    >
                      <lang.Icon size="1em" color={lang.color} />
                    </span>
                  ) : (
                    <span
                      style={{
                        color: ACCENT,
                        fontFamily: MONO,
                        fontSize: 'clamp(20px, 5vw, 36px)',
                        fontWeight: 500,
                        lineHeight: 1,
                      }}
                    >
                      {lang.glyph}
                    </span>
                  )}
                </div>
                <span
                  style={{
                    fontSize: 'clamp(8.5px, 2.4vw, 13px)',
                    fontWeight: 500,
                    letterSpacing: '0.01em',
                    color: INK,
                    textAlign: 'center',
                    lineHeight: 1.2,
                    maxWidth: '100%',
                    overflowWrap: 'break-word',
                  }}
                >
                  {lang.name}
                </span>
              </motion.div>
            ))}
          </div>
        </div>
      </div>

      <style>{`
        .lang-tile:hover {
          box-shadow: 0 16px 36px -20px rgba(109,40,217,0.32);
          transform: translateY(-3px);
          border-color: ${ACCENT};
        }
      `}</style>
    </section>
  );
}
