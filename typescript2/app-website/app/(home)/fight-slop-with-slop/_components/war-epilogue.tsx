import Image from 'next/image';
import { section } from '@/components/home/analytics';
import { PAGE_WRAP } from '@/components/home/page-shell';

export default function WarEpilogue() {
  return (
    <section
      {...section('slop_epilogue')}
      className={`${PAGE_WRAP} pt-16 md:pt-24 pb-8 text-center`}
    >
      <div className="mx-auto max-w-3xl">
        <h2 className="v4-h2">But we&apos;re losing the war</h2>

        <p className="mx-auto mt-10 max-w-2xl v4-body text-ink-2">
          You are only as good as the invariants the underlying systems hold.
        </p>

        <figure className="mt-10">
          <Image
            alt="TypeScript design goals highlighting that a provably correct type system is a non-goal"
            className="mx-auto h-auto w-full max-w-2xl rounded-2xl border border-rule"
            height={674}
            sizes="(min-width: 768px) 672px, 100vw"
            src="/war-on-slop/invariants-quote.png"
            width={1268}
          />
        </figure>

        <h2 className="mt-16 md:mt-24 v4-h2 text-accent">We need BAML</h2>

        <p className="mx-auto mt-10 max-w-2xl font-display text-[24px] md:text-[32px] font-medium leading-[1.3] tracking-[-0.02em] text-head">
          {'Our bet at '}
          <span className="text-accent">BAML</span>
          {
            ': the best way to reduce slop is by developing clean constructs at the language level'
          }
        </p>

        <p className="mx-auto mt-10 max-w-2xl v4-body text-body">
          Slop shouldn&apos;t win. But we shouldn&apos;t ignore slop. Let&apos;s
          just build beautiful software.
        </p>
      </div>
    </section>
  );
}
