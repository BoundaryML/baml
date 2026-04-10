'use client';

import { useEffect, useRef, useState } from 'react';
import { useMotionValueEvent, useScroll } from 'motion/react';
import ParticleImage from '@/components/ParticleImage';
import { siteConfig } from '@/app/_lib/config';

// ── Data ──────────────────────────────────────────────────────────────────────

const timeline = siteConfig.storyPage.timeline;
const ERAS = timeline.length;

const ERA_IMAGES: Record<string, string> = {
  'C, Unix': '/Sphinx Icon.png',
  'Java, JavaScript': '/Satellite Icon.png',
  'TypeScript': '/Bridge Icon.png',
  'Python': '/Telescope Icon.png',
  'BAML': '/Constellation Icon.png',
};

const DASH_MASK =
  'repeating-linear-gradient(to bottom, black 0px, black 4px, transparent 4px, transparent 18px)';

// ── Particle image — mounts once when era first becomes active ────────────────

function LazyParticleImage({
  sectionActive,
  ...props
}: React.ComponentProps<typeof ParticleImage> & { sectionActive: boolean }) {
  const [hasTriggered, setHasTriggered] = useState(false);

  useEffect(() => {
    if (sectionActive && !hasTriggered) setHasTriggered(true);
  }, [sectionActive, hasTriggered]);

  return (
    <div style={{ width: props.width, height: props.height }}>
      {hasTriggered && <ParticleImage {...props} />}
    </div>
  );
}

// ── Era card ──────────────────────────────────────────────────────────────────

function EraCard({
  node,
  index,
  isActive,
}: {
  node: (typeof timeline)[number];
  index: number;
  isActive: boolean;
}) {
  const image = ERA_IMAGES[node.label];
  const isOdd = index % 2 !== 0;
  const isBaml = node.label === 'BAML';

  const textCol = (
    <div
      style={{
        flex: '0 0 60%',
        maxWidth: '60%',
        paddingLeft: isOdd
          ? node.label === 'Java, JavaScript' || node.label === 'Python' ? 96 : 64
          : 0,
        paddingRight: isOdd ? 0 : 64,
      }}
    >
      <h2
        className="font-bold"
        style={{
          fontSize: isBaml ? 'clamp(2rem, 5vw, 3rem)' : 'clamp(1.5rem, 3vw, 2.25rem)',
          color: isBaml ? '#8b5cf6' : undefined,
          letterSpacing: isBaml ? '0.06em' : '-0.02em',
        }}
      >
        {node.label}
      </h2>
      <p
        className="text-sm font-normal leading-relaxed text-muted-foreground"
        style={{ marginTop: 12, maxWidth: 320 }}
      >
        {node.body}
      </p>
    </div>
  );

  const iconCol = (
    <div
      style={{
        flex: '0 0 40%',
        maxWidth: '40%',
        paddingLeft: isOdd
          ? 0
          : node.label === 'C, Unix' || node.label === 'TypeScript' ? 36 : 64,
        paddingRight: isOdd
          ? node.label === 'C, Unix' || node.label === 'TypeScript' ? 36 : 64
          : 0,
        paddingTop: 40,
        paddingBottom: 40,
        display: 'flex',
        justifyContent: 'center',
        backgroundColor: 'var(--background)',
        marginTop: -40,
      }}
    >
      {image && (
        <div style={{ maxWidth: 400, width: '100%', display: 'flex', justifyContent: 'center' }}>
          <div
            style={
              isBaml
                ? {
                    animation: isActive ? 'baml-pulse 2.4s ease-in-out infinite' : 'none',
                    filter: 'drop-shadow(0 0 12px rgba(139,92,246,0.45))',
                  }
                : undefined
            }
          >
            <LazyParticleImage
              src={image}
              width={350}
              height={350}
              jitter={0.15}
              hoverRepel={5}
              density={6}
              particleSize={3}
              streamDuration={2.0}
              streamStyle={isBaml ? 'radial' : 'cascade'}
              spring={0.02}
              sectionActive={isActive}
            />
          </div>
        </div>
      )}
    </div>
  );

  return (
    <div className="relative z-10 mx-auto w-full max-w-4xl">
      {isBaml && (
        <div
          aria-hidden
          className="pointer-events-none absolute inset-0 rounded-2xl"
          style={{
            background: 'radial-gradient(ellipse at center, rgba(139,92,246,0.08) 0%, transparent 75%)',
          }}
        />
      )}
      <div className="flex flex-col gap-4 sm:flex-row sm:items-center">
        {isOdd ? <>{iconCol}{textCol}</> : <>{textCol}{iconCol}</>}
      </div>
    </div>
  );
}

// ── Root export ───────────────────────────────────────────────────────────────

export function StoryTimeline() {
  const containerRef = useRef<HTMLDivElement>(null);
  const [activeIndex, setActiveIndex] = useState(0);
  const isBamlActive = activeIndex === ERAS - 1;

  const { scrollYProgress } = useScroll({
    target: containerRef,
    offset: ['start start', 'end end'],
  });

  useMotionValueEvent(scrollYProgress, 'change', (v) => {
    setActiveIndex(Math.min(ERAS - 1, Math.floor(v * ERAS)));
  });

  return (
    <div ref={containerRef} style={{ height: `${ERAS * 100}vh`, position: 'relative' }}>
      <style>{`
        @keyframes baml-pulse {
          0%, 100% { filter: drop-shadow(0 0 12px rgba(139,92,246,0.45)); }
          50%       { filter: drop-shadow(0 0 28px rgba(139,92,246,0.85)); }
        }
      `}</style>

      {/* Sticky full-viewport panel */}
      <div
        className="sticky top-0 w-full overflow-hidden bg-background"
        style={{ height: '100vh' }}
      >
        {/* Header */}
        <div className="px-4 pt-8 sm:px-8 md:px-12">
          <p className="text-xs font-semibold uppercase tracking-[0.2em] text-muted-foreground">
            A brief history of languages
          </p>
        </div>

        {/* Spine — two layers crossfading to purple when BAML is active */}
        <div
          className="pointer-events-none absolute left-1/2 -translate-x-1/2"
          style={{ top: 0, bottom: 0, width: 1 }}
        >
          <div
            className="absolute inset-0"
            style={{
              background: 'linear-gradient(to bottom, rgba(209,213,219,0.9) 0%, rgba(209,213,219,0.9) 55%, rgba(139,92,246,0.9) 80%, rgba(139,92,246,1) 100%)',
              maskImage: DASH_MASK,
              WebkitMaskImage: DASH_MASK,
              opacity: isBamlActive ? 0 : 1,
              transition: 'opacity 1000ms ease',
            }}
          />
          <div
            className="absolute inset-0"
            style={{
              background: 'linear-gradient(to bottom, rgba(139,92,246,0.4) 0%, rgba(139,92,246,1) 100%)',
              maskImage: DASH_MASK,
              WebkitMaskImage: DASH_MASK,
              opacity: isBamlActive ? 1 : 0,
              transition: 'opacity 1000ms ease',
            }}
          />
        </div>

        {/* Era panels — absolutely stacked, one visible at a time */}
        <div className="relative px-4 sm:px-8 md:px-12" style={{ height: 'calc(100vh - 80px)' }}>
          {timeline.map((node, i) => (
            <div
              key={node.era}
              style={{
                position: 'absolute',
                inset: 0,
                display: 'flex',
                alignItems: 'center',
                opacity: activeIndex === i ? 1 : 0,
                transform:
                  activeIndex === i
                    ? 'translateY(0)'
                    : activeIndex > i
                      ? 'translateY(-32px)'
                      : 'translateY(32px)',
                transition: 'opacity 500ms ease, transform 500ms ease',
                pointerEvents: activeIndex === i ? 'auto' : 'none',
              }}
            >
              <EraCard node={node} index={i} isActive={activeIndex === i} />
            </div>
          ))}
        </div>

        {/* Progress dots */}
        <div className="absolute bottom-8 left-1/2 -translate-x-1/2 flex gap-2.5">
          {timeline.map((_, i) => (
            <div
              key={timeline[i].era}
              style={{
                width: 6,
                height: 6,
                borderRadius: '50%',
                backgroundColor:
                  i < activeIndex
                    ? 'rgba(139,92,246,0.4)'
                    : i === activeIndex
                      ? isBamlActive ? '#8b5cf6' : 'rgba(26,26,26,0.7)'
                      : 'rgba(209,213,219,0.6)',
                transition: 'background-color 300ms ease',
              }}
            />
          ))}
        </div>

        {/* Scroll cue */}
        {activeIndex < ERAS - 1 && (
          <p className="absolute bottom-8 right-8 text-xs text-muted-foreground select-none">
            scroll to continue ↓
          </p>
        )}
      </div>
    </div>
  );
}
