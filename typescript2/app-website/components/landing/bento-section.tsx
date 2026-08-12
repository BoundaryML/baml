'use client';

import { siteConfig } from '@/app/_lib/config';
import { SectionHeader } from '@/components/section-header';

export function BentoSection() {
  const { title, description, items } = siteConfig.bentoSection;

  return (
    <section
      className="flex flex-col items-center justify-center w-full relative px-2 md:px-10"
      id="bento"
    >
      <div className="mx-2 md:mx-10 relative">
        {/* <div className="absolute top-0 -left-4 md:-left-14 h-full w-4 md:w-14 text-primary/5 bg-[size:10px_10px] [background-image:repeating-linear-gradient(315deg,currentColor_0_1px,#0000_0_50%)]" /> */}
        {/* <div className="absolute top-0 -right-4 md:-right-14 h-full w-4 md:w-14 text-primary/5 bg-[size:10px_10px] [background-image:repeating-linear-gradient(315deg,currentColor_0_1px,#0000_0_50%)]" /> */}

        <SectionHeader>
          <h2 className="text-3xl md:text-4xl font-medium tracking-tighter text-center text-balance pb-1">
            {title}
          </h2>
          <p className="text-muted-foreground text-center text-balance font-medium">
            {description}
          </p>
        </SectionHeader>

        <div className="grid grid-cols-1 md:grid-cols-2 overflow-hidden">
          {items.map((item) => (
            <div
              className="flex min-h-[500px] flex-col rounded-lg border border-border bg-card/30 p-0.5 md:min-h-[480px]"
              key={item.id}
            >
              <div className="relative flex min-h-[280px] flex-1 items-center justify-center overflow-hidden md:min-h-[260px]">
                {item.content}
              </div>
              <div className="flex flex-shrink-0 flex-col gap-2 p-6">
                <h3 className="text-lg tracking-tighter font-semibold">
                  {item.title}
                </h3>
                <p className="text-muted-foreground">{item.description}</p>
              </div>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
