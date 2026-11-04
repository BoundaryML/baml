import { section } from '../analytics';

const SIDES = [
  {
    body: "Write one function in BAML and call it from your Python, TypeScript, Go, Rust, Java, Swift, or C++ app through a generated client. You don't need to rewrite your app or run a separate service.",
    title: 'Start with the language.',
  },
  {
    body: 'If you already send OpenTelemetry traces, send them to Boundary and query them with baml query. Move code into BAML when you want typed telemetry, program versioning, and better developer experience.',
    title: 'Start with production.',
  },
];

/** Section 5: either half of the stack can come first. */
export default function Adopt() {
  return (
    <section
      {...section('adopt')}
      aria-labelledby="adopt"
      className="border-t border-white/[0.08]"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) py-20 md:py-28">
        <h2 className="v4-h2" id="adopt">
          Start with either side of the stack.
        </h2>
        <p className="mt-5 text-lg text-white/70">
          You don't have to move everything at once.
        </p>
        <div className="mt-10 md:mt-12 grid md:grid-cols-2 gap-3 md:gap-4">
          {SIDES.map((s, i) => (
            <div
              className="rounded-2xl border border-white/10 bg-white/[0.03] p-6 md:p-8"
              key={s.title}
            >
              {/* Which side of the hero's deploy line this starts from. */}
              <p className="text-[13px] text-white/45">
                {i === 0 ? 'Build and test' : 'Production'}
              </p>
              <h3 className="mt-3 font-display font-medium text-[22px] md:text-[26px] tracking-[-0.02em] text-white">
                {s.title}
              </h3>
              <p className="mt-3 text-[16px] md:text-[17px] leading-[1.55] text-white/70">
                {s.body}
              </p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
