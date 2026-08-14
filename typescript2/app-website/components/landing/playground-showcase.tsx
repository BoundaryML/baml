'use client';

import dynamic from 'next/dynamic';
import Image from 'next/image';
import Link from 'next/link';
import { useIsDesktop } from '@/hooks/use-media-query';

const BamlPlayground = dynamic(
  () => import('@/playground/BamlPlayground').then((m) => m.BamlPlayground),
  {
    loading: () => (
      <div className="flex h-full min-h-[520px] items-center justify-center text-sm text-[#5C5852]">
        Loading playground...
      </div>
    ),
    ssr: false,
  },
);

export function PlaygroundShowcase() {
  const isDesktop = useIsDesktop();

  return (
    <section
      aria-label="Interactive BAML playground"
      className="w-full border-t border-[#D9D3C4] bg-[#FBF7ED] px-4 py-16 sm:px-8 lg:py-24"
    >
      <div className="mx-auto flex w-full max-w-[100rem] flex-col gap-8">
        <div className="grid gap-5 lg:grid-cols-[minmax(0,0.72fr)_minmax(280px,0.28fr)] lg:items-end">
          <div>
            <p className="m-0 text-[13px] font-medium uppercase tracking-[0.12em] text-[#8A8580]">
              Live playground
            </p>
            <h2 className="m-0 mt-3 max-w-4xl text-balance text-[clamp(2rem,4vw,3.5rem)] font-semibold leading-[1.08] tracking-[-0.025em] text-[#1A1612]">
              Try BAML right here in your browser.
            </h2>
          </div>
        </div>

        <div className="overflow-hidden rounded-lg border border-[#D9D3C4] bg-white shadow-[0_24px_80px_-44px_rgba(26,22,18,0.35)]">
          <div className="grid h-10 grid-cols-[88px_1fr_88px] items-center border-b border-[#D9D3C4] bg-[#F5F1E5] px-4">
            <div className="flex gap-1.5">
              <span className="h-2.5 w-2.5 rounded-full border border-black/10 bg-[#E5A8A8]" />
              <span className="h-2.5 w-2.5 rounded-full border border-black/10 bg-[#E5C58A]" />
              <span className="h-2.5 w-2.5 rounded-full border border-black/10 bg-[#A8D0A0]" />
            </div>
            <div className="text-center font-mono text-[11px] tracking-[0.04em] text-[#5C5852]">
              BAML Playground
            </div>
            <Link
              className="justify-self-end text-[12px] text-[#5C5852] no-underline transition-colors hover:text-[#6D28D9]"
              href="/how-the-playground-works"
            >
              how it works
            </Link>
          </div>
          <div className="relative h-[min(760px,82vh)] min-h-[560px]">
            {isDesktop ? (
              <BamlPlayground />
            ) : (
              <Link
                className="group relative block h-full w-full"
                href="/how-the-playground-works"
              >
                <Image
                  alt="BAML playground preview"
                  className="object-cover object-top"
                  fill
                  sizes="100vw"
                  src="/bamlPlaygroundLightScreenshot.png"
                />
                <div className="absolute inset-x-0 bottom-0 bg-white/90 px-4 py-3 text-center text-[13px] text-[#1A1612]">
                  Open the playground on a desktop browser to try it live
                </div>
              </Link>
            )}
          </div>
        </div>
      </div>
    </section>
  );
}
