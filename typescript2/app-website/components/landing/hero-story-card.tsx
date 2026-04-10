'use client';

import { useInView } from 'framer-motion';
import Link from 'next/link';
import { useRef } from 'react';
import { siteConfig } from '@/app/_lib/config';
import { BorderBeam } from '@/components/magicui/border-beam';
import { Button } from '@/components/ui/button';

export function HeroStoryCard() {
  const ref = useRef(null);
  const inView = useInView(ref, { margin: '-50px', once: true });
  const { timeline, ctaText, ctaHref } = siteConfig.storySection;

  return (
    <div
      className="relative animate-fade-up opacity-0 [--animation-delay:400ms] [perspective:2000px]"
      ref={ref}
    >
      <div
        className={`rounded-xl border border-border bg-card/50 backdrop-blur-sm before:absolute before:bottom-1/2 before:left-0 before:top-0 before:h-full before:w-full before:opacity-0 before:[filter:blur(180px)] before:[background-image:linear-gradient(to_bottom,var(--secondary),var(--secondary),transparent_40%)] ${
          inView ? 'before:animate-image-glow' : ''
        }`}
      >
        <BorderBeam
          colorFrom="var(--color-one)"
          colorTo="var(--secondary)"
          delay={11}
          duration={12}
          size={200}
        />
        <div className="relative z-10 p-6 sm:p-8 flex flex-col min-h-[280px] sm:min-h-[360px]">
          <p className="text-xs font-medium uppercase tracking-widest text-muted-foreground mb-4">
            A language for the future of AI
          </p>
          {/* Timeline */}
          <div className="flex flex-col gap-4 flex-1">
            {timeline.map((node, i) => (
              <div
                className="flex items-center gap-3 text-left"
                key={node.era}
                style={{
                  animationDelay: `${500 + i * 80}ms`,
                }}
              >
                <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-secondary/20 text-xs font-semibold text-secondary">
                  {i + 1}
                </span>
                <div className="min-w-0">
                  <span className="text-xs text-muted-foreground">
                    {node.era}
                  </span>
                  <p className="font-semibold text-foreground truncate">
                    {node.label}
                  </p>
                </div>
              </div>
            ))}
          </div>
          <div className="mt-6 pt-6 border-t border-border">
            <p className="text-sm text-muted-foreground mb-4">
              Code that agents write. Software that humans trust.
            </p>
            <Button asChild className="w-full sm:w-auto" variant="secondary">
              <Link href={ctaHref}>{ctaText}</Link>
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
