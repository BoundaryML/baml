'use client';

import { siteConfig } from '@/app/_lib/config';
import { SectionHeader } from '@/components/section-header';

export function FeatureSection() {
  const { title, description, items } = siteConfig.featureSection;

  return (
    <section
      className="flex flex-col items-center justify-center w-full relative"
      id="features"
    >
      <SectionHeader>
        <h2 className="text-3xl md:text-4xl font-medium tracking-tighter text-center text-balance">
          {title}
        </h2>
        <p className="text-muted-foreground text-center text-balance font-medium">
          {description}
        </p>
      </SectionHeader>

      <div className="w-full max-w-5xl mx-auto px-6 grid grid-cols-1 md:grid-cols-2 gap-6">
        {items.map((item, index) => (
          <div
            key={item.id}
            className="group relative flex flex-col rounded-xl border border-border overflow-hidden bg-background"
          >
            {/* Media */}
            <div className="relative w-full aspect-video bg-muted/30 overflow-hidden">
              {item.video && (
                <video
                  autoPlay
                  className="w-full h-full object-cover"
                  loop
                  muted
                  playsInline
                  preload="auto"
                  src={item.video}
                />
              )}
              {item.component && (
                <div className="w-full h-full">
                  {item.component}
                </div>
              )}
            </div>

            {/* Text */}
            <div className="p-5">
              <div className="flex items-baseline gap-3 mb-2">
                <span className="text-xs font-medium text-muted-foreground/50">
                  {String(index + 1).padStart(2, '0')}
                </span>
                <h3 className="text-lg font-semibold tracking-tight">
                  {item.title}
                </h3>
              </div>
              <div className="text-sm text-muted-foreground leading-relaxed">
                {item.content}
              </div>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
