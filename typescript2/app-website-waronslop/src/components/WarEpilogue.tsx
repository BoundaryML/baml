export default function WarEpilogue() {
  return (
    <section className="tweet-font mx-auto max-w-3xl px-6 pb-8 pt-16 sm:pt-24">
      <h2 className="text-center text-3xl font-bold tracking-tight text-ink sm:text-5xl">
        But we&apos;re losing the war
      </h2>

      <p className="mx-auto mt-10 max-w-2xl text-center text-xl font-medium leading-relaxed text-ink sm:text-2xl">
        You are only as good as the invariants the underlying systems hold.
      </p>

      <figure className="mt-10">
        <img
          src="/invariants-quote.png"
          alt="TypeScript design goals highlighting that a provably correct type system is a non-goal"
          className="mx-auto w-full max-w-2xl rounded-xl border border-line shadow-sm"
        />
      </figure>

      <p className="mx-auto mt-16 max-w-2xl text-center text-2xl font-medium leading-relaxed text-ink sm:text-[2rem] sm:leading-snug">
        {'Our bet at '}
        <span className="font-bold text-accent">BAML</span>
        {': the best way to reduce slop is by developing clean constructs at the language level'}
      </p>

      <p className="mx-auto mt-10 max-w-2xl text-center text-lg leading-relaxed text-ink-2 sm:text-xl">
        Slop shouldn&apos;t win. But we also can&apos;t ignore slop. Let&apos;s just build beautiful
        software.
      </p>
    </section>
  );
}
