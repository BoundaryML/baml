import type { ReactNode } from 'react';
import { section } from './analytics';

// Section 8: BAML can be adopted a piece at a time, starting from either end: the language or
// production.

const LANGUAGES = 'Python, TypeScript, Go, Rust, Java, Swift, or C++';

export default function Adopt() {
  return (
    <section
      {...section('adopt')}
      aria-labelledby="adopt"
      className="border-t border-rule"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) py-20 md:py-28">
        <h2 className="v4-h2" id="adopt">
          Adopt BAML incrementally.
        </h2>
        <p className="mt-5 v4-body text-ink-2">
          Start with the language or with production. You don't have to move
          everything at once.
        </p>
        <div className="mt-10 md:mt-12 grid md:grid-cols-2 gap-3 md:gap-4">
          <Side title="Start with the language.">
            Write one function in BAML and call it from your {LANGUAGES} app
            through a generated client.
          </Side>
          <Side title="Start with production.">
            Send the OpenTelemetry traces you already have to Boundary, and move
            code into BAML when you want typed telemetry.
          </Side>
        </div>
      </div>
    </section>
  );
}

function Side({ children, title }: { children: ReactNode; title: string }) {
  return (
    <div className="rounded-2xl border border-rule bg-raised p-6 md:p-8">
      <h3 className="v4-h3 text-head">{title}</h3>
      <p className="mt-4 v4-body text-ink-2">{children}</p>
    </div>
  );
}
