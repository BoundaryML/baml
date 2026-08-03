'use client';

import { useEffect, useRef, useState } from 'react';

interface ScrollVideoProps {
  src: string;
  /** How tall the scroll-driven section is, as a multiple of the viewport height. */
  scrollHeightVh?: number;
}

export function ScrollVideo({ src, scrollHeightVh = 500 }: ScrollVideoProps) {
  const sectionRef = useRef<HTMLDivElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const targetTimeRef = useRef(0);
  const currentTimeRef = useRef(0);
  const durationRef = useRef(0);
  const rafRef = useRef<number | null>(null);
  const seekingRef = useRef(false);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    const section = sectionRef.current;
    const video = videoRef.current;
    if (!section || !video) return;

    let cancelled = false;

    const onLoaded = () => {
      if (cancelled) return;
      durationRef.current = Number.isFinite(video.duration)
        ? video.duration
        : 0;
      setReady(true);
      updateTarget();
    };

    const tick = () => {
      const target = targetTimeRef.current;
      const current = currentTimeRef.current;
      const diff = target - current;
      if (Math.abs(diff) < 0.01) {
        currentTimeRef.current = target;
      } else {
        currentTimeRef.current = current + diff * 0.25;
      }
      if (!seekingRef.current) {
        seekingRef.current = true;
        try {
          video.currentTime = currentTimeRef.current;
        } catch {
          /* ignore */
        }
      }
      if (Math.abs(target - currentTimeRef.current) >= 0.01) {
        rafRef.current = requestAnimationFrame(tick);
      } else {
        rafRef.current = null;
      }
    };

    const onSeeked = () => {
      seekingRef.current = false;
    };

    const updateTarget = () => {
      const duration = durationRef.current;
      if (duration <= 0) return;
      const rect = section.getBoundingClientRect();
      const viewport = window.innerHeight;
      const totalScroll = rect.height - viewport;
      if (totalScroll <= 0) return;
      const scrolled = Math.min(Math.max(-rect.top, 0), totalScroll);
      const progress = scrolled / totalScroll;
      targetTimeRef.current = progress * Math.max(0, duration - 1 / 30);
      if (rafRef.current == null) rafRef.current = requestAnimationFrame(tick);
    };

    video.addEventListener('loadedmetadata', onLoaded);
    video.addEventListener('seeked', onSeeked);
    window.addEventListener('scroll', updateTarget, { passive: true });
    window.addEventListener('resize', updateTarget);

    if (video.readyState >= 1) onLoaded();

    return () => {
      cancelled = true;
      video.removeEventListener('loadedmetadata', onLoaded);
      video.removeEventListener('seeked', onSeeked);
      window.removeEventListener('scroll', updateTarget);
      window.removeEventListener('resize', updateTarget);
      if (rafRef.current != null) cancelAnimationFrame(rafRef.current);
    };
  }, [src]);

  return (
    <section
      ref={sectionRef}
      style={{
        position: 'relative',
        height: `${scrollHeightVh}vh`,
        backgroundColor: '#DCC9F5',
      }}
    >
      <div
        style={{
          position: 'sticky',
          top: 0,
          height: '100vh',
          width: '100%',
          overflow: 'hidden',
        }}
      >
        <video
          ref={videoRef}
          src={src}
          muted
          playsInline
          preload="auto"
          // biome-ignore lint/a11y/useMediaCaption: decorative scroll-scrubbed visual
          style={{
            position: 'absolute',
            inset: 0,
            width: '100%',
            height: '100%',
            objectFit: 'cover',
            filter: 'grayscale(1) contrast(1.05) brightness(1.02)',
            mixBlendMode: 'multiply',
            opacity: ready ? 0.45 : 0,
            transition: 'opacity 300ms ease',
          }}
        />
        <div
          style={{
            position: 'absolute',
            inset: 0,
            backgroundColor: '#6D28D9',
            mixBlendMode: 'multiply',
            opacity: 0.15,
            pointerEvents: 'none',
          }}
        />
        <div
          style={{
            position: 'absolute',
            top: '48px',
            left: '48px',
            textAlign: 'left',
            color: '#1A1612',
            fontSize: '22px',
            lineHeight: 1.35,
            fontWeight: 500,
            letterSpacing: '-0.01em',
            pointerEvents: 'none',
          }}
        >
          <p style={{ margin: 0, whiteSpace: 'nowrap' }}>
            baml was built for agents
          </p>
        </div>
      </div>
    </section>
  );
}
