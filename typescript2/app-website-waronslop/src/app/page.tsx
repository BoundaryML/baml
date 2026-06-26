import WarScene from '@/components/WarScene';
import PledgeForm from '@/components/PledgeForm';
import PledgeWall from '@/components/PledgeWall';

export default function Home() {
  return (
    <main className="w-full overflow-hidden">
      <section className="relative z-10 px-6 pt-20 text-center sm:pt-28">
        <h1 className="text-4xl font-bold tracking-tight text-ink sm:text-6xl">fight slop with slop</h1>
        <p className="tweet-font mt-3 text-sm text-accent">
          by{' '}
          <a href="https://x.com/boundaryml" target="_blank" rel="noopener noreferrer" className="hover:underline">
            @boundaryml
          </a>
        </p>
      </section>

      <WarScene />

      <section className="mx-auto max-w-2xl px-6 pb-24 pt-4 sm:pb-32 sm:pt-8">
        <p className="tweet-font text-center text-2xl font-medium leading-relaxed text-ink sm:text-[2rem] sm:leading-snug">
          {'Our bet at '}
          <span className="font-bold text-accent">BAML</span>
          {': the best way to reduce slop is by developing clean constructs at the language level'}
        </p>
      </section>

      <section className="pb-16 sm:pb-20">
        <PledgeWall />
      </section>

      <section className="mx-auto max-w-2xl px-6 pb-28">
        <h2 className="mb-8 text-center text-3xl font-bold tracking-tight text-ink sm:text-4xl">Share</h2>
        <PledgeForm />
      </section>
    </main>
  );
}
