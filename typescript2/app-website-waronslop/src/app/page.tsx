import WarScene from '@/components/WarScene';
import PledgeForm from '@/components/PledgeForm';
import PledgeWall from '@/components/PledgeWall';

export default function Home() {
  return (
    <main className="w-full">
      {/* 1. Scroll-driven war hero (pinned; the column marches as you scroll) */}
      <WarScene />

      {/* 2. The bet we're making at baml */}
      <section className="mx-auto max-w-2xl px-6 py-24 sm:py-32">
        <p
          className="text-center text-2xl leading-relaxed text-ink sm:text-[2rem] sm:leading-snug"
          style={{ fontFamily: "'Times New Roman', Times, serif" }}
        >
          {"We're making a bet at "}
          <span className="font-bold text-accent">baml</span>
          {': the best way to reduce slop is by developing core constructs at the language level.'}
        </p>
      </section>

      {/* 3. How other people are fighting slop — the live wall */}
      <section className="pb-16 sm:pb-20">
        <h2 className="mb-6 px-6 text-center text-xs font-bold uppercase tracking-[0.2em] text-ink-2 sm:mb-8">
          how others are fighting slop
        </h2>
        <PledgeWall />
      </section>

      {/* 4. Add yours */}
      <section className="mx-auto max-w-2xl px-6 pb-28">
        <h2 className="mb-8 text-center text-2xl font-bold tracking-tight text-ink sm:text-3xl">
          fight slop with slop
        </h2>
        <PledgeForm />
      </section>
    </main>
  );
}
